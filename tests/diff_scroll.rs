use git2::{Oid, Repository, RepositoryInitOptions, Signature, Time};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use tempfile::TempDir;

use ravix::app::{Action, App};
use ravix::ui::render;

const NOW: i64 = 100_000;

fn commit(repo: &Repository, base: Option<Oid>, files: &[(&str, String)], message: &str) -> Oid {
    let signature = Signature::new("Dev", "dev@example.com", &Time::new(1000, 0)).unwrap();
    let base_tree = base.map(|oid| repo.find_commit(oid).unwrap().tree().unwrap());
    let mut builder = repo.treebuilder(base_tree.as_ref()).unwrap();
    for (path, content) in files {
        let blob = repo.blob(content.as_bytes()).unwrap();
        builder.insert(*path, blob, 0o100644).unwrap();
    }
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let parents: Vec<_> = base
        .into_iter()
        .map(|oid| repo.find_commit(oid).unwrap())
        .collect();
    let parent_refs: Vec<_> = parents.iter().collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parent_refs,
    )
    .unwrap()
}

fn long_body(marker: &str) -> String {
    let mut body = String::new();
    for index in 0..120 {
        body.push_str(&format!("{marker} line {index}\n"));
    }
    body.push_str(&format!("{marker} LASTMARKER\n"));
    body
}

fn long_diff_repo(dir: &TempDir) {
    let mut opts = RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(dir.path(), &opts).unwrap();

    let root = commit(&repo, None, &[("seed.txt", "seed\n".to_string())], "seed");
    commit(
        &repo,
        Some(root),
        &[
            ("alpha.txt", long_body("alpha")),
            ("beta.txt", long_body("beta")),
        ],
        "add two long files",
    );
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
}

fn enter_commit_fullscreen(app: &mut App) {
    app.update(Action::OpenPanel);
    app.update(Action::OpenPanel);
}

fn draw(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, app, NOW)).unwrap();
    terminal.backend().buffer().clone()
}

fn dump(buffer: &Buffer) -> String {
    let area = buffer.area();
    let mut out = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            out.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn scrolling_down_in_the_fullscreen_diff_moves_the_offset() {
    let dir = TempDir::new().unwrap();
    long_diff_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    enter_commit_fullscreen(&mut app);
    assert!(
        app.panel().unwrap().diff.is_some(),
        "the fullscreen diff loads the first file"
    );
    assert_eq!(app.panel().unwrap().diff_scroll, 0);

    app.update(Action::ScrollDown);
    assert!(
        app.panel().unwrap().diff_scroll > 0,
        "scrolling down advances the diff offset instead of the graph"
    );
}

#[test]
fn scrolling_up_never_underflows_past_the_top() {
    let dir = TempDir::new().unwrap();
    long_diff_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    enter_commit_fullscreen(&mut app);
    for _ in 0..5 {
        app.update(Action::ScrollUp);
    }
    assert_eq!(
        app.panel().unwrap().diff_scroll,
        0,
        "the offset stays pinned at the top"
    );
}

#[test]
fn changing_file_resets_the_scroll_offset() {
    let dir = TempDir::new().unwrap();
    long_diff_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    enter_commit_fullscreen(&mut app);
    for _ in 0..4 {
        app.update(Action::ScrollDown);
    }
    assert!(app.panel().unwrap().diff_scroll > 0);

    app.update(Action::SelectNext);
    assert_eq!(
        app.panel().unwrap().diff_scroll,
        0,
        "moving to another file starts back at the top"
    );
}

#[test]
fn toggling_to_side_by_side_resets_the_scroll_so_it_stays_navigable() {
    let dir = TempDir::new().unwrap();
    long_diff_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    enter_commit_fullscreen(&mut app);
    for _ in 0..6 {
        app.update(Action::ScrollDown);
    }
    assert!(
        app.panel().unwrap().diff_scroll > 0,
        "scrolled down in unified"
    );

    app.update(Action::ToggleDiffView);
    assert_eq!(
        app.panel().unwrap().diff_scroll,
        0,
        "switching to side-by-side snaps back to the top instead of a stale offset"
    );

    draw(&mut app, 120, 24);
    app.update(Action::ScrollDown);
    assert!(
        app.panel().unwrap().diff_scroll > 0,
        "the side-by-side diff is still scrollable"
    );
}

#[test]
fn over_scrolling_clamps_to_the_end_and_reveals_the_last_line() {
    let dir = TempDir::new().unwrap();
    long_diff_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    enter_commit_fullscreen(&mut app);
    draw(&mut app, 120, 24);
    for _ in 0..400 {
        app.update(Action::ScrollDown);
    }
    let buffer = draw(&mut app, 120, 24);

    assert!(
        app.panel().unwrap().diff_scroll < 200,
        "the offset is clamped to the content height, not left running away"
    );
    assert!(
        dump(&buffer).contains("LASTMARKER"),
        "scrolling to the end reveals the final diff line"
    );
}
