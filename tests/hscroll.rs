use git2::{Oid, Repository, RepositoryInitOptions, Signature, Time};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use tempfile::TempDir;

use ravix::app::{Action, App};
use ravix::ui::render;

const NOW: i64 = 100_000;

fn commit(repo: &Repository, base: Option<Oid>, path: &str, body: &str) -> Oid {
    let signature = Signature::new("Dev", "dev@example.com", &Time::new(1000, 0)).unwrap();
    let blob = repo.blob(body.as_bytes()).unwrap();
    let base_tree = base.map(|oid| repo.find_commit(oid).unwrap().tree().unwrap());
    let mut builder = repo.treebuilder(base_tree.as_ref()).unwrap();
    builder.insert(path, blob, 0o100644).unwrap();
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let parents: Vec<_> = base
        .into_iter()
        .map(|oid| repo.find_commit(oid).unwrap())
        .collect();
    let refs: Vec<_> = parents.iter().collect();
    repo.commit(Some("HEAD"), &signature, &signature, "change", &tree, &refs)
        .unwrap()
}

fn wide_line_repo(dir: &TempDir) {
    let mut opts = RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(dir.path(), &opts).unwrap();
    let root = commit(&repo, None, "a.txt", "alpha\nvalue\nomega\n");
    let long = format!("value = {} TAILMARKER", "x".repeat(90));
    commit(&repo, Some(root), "a.txt", &format!("alpha\n{long}\nomega\n"));
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
}

fn draw(app: &mut App, w: u16, h: u16) -> Buffer {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render(f, app, NOW)).unwrap();
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

fn enter_split_diff(app: &mut App) {
    app.update(Action::OpenPanel);
    app.update(Action::OpenPanel); // fullscreen
    app.update(Action::ToggleDiffView); // side-by-side
    app.update(Action::ToggleFocus); // focus the diff
}

#[test]
fn horizontal_scroll_reveals_the_tail_of_a_long_line_in_side_by_side() {
    let dir = TempDir::new().unwrap();
    wide_line_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    enter_split_diff(&mut app);
    let before = dump(&draw(&mut app, 120, 24));
    assert!(
        !before.contains("TAILMARKER"),
        "the tail of the long line is off-screen before scrolling:\n{before}"
    );

    for _ in 0..60 {
        app.update(Action::ScrollDiffRight);
    }
    let after = dump(&draw(&mut app, 120, 24));
    assert!(
        app.panel().unwrap().diff_hscroll > 0,
        "scrolling right advances the horizontal offset"
    );
    assert!(
        after.contains("TAILMARKER"),
        "scrolling right reveals the tail of the long line:\n{after}"
    );
}

#[test]
fn switching_back_to_unified_clears_the_horizontal_offset() {
    let dir = TempDir::new().unwrap();
    wide_line_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    enter_split_diff(&mut app);
    for _ in 0..20 {
        app.update(Action::ScrollDiffRight);
    }
    assert!(app.panel().unwrap().diff_hscroll > 0);

    app.update(Action::ToggleDiffView); // back to unified
    draw(&mut app, 120, 24);
    assert_eq!(
        app.panel().unwrap().diff_hscroll,
        0,
        "unified wraps, so there is no horizontal offset"
    );
}

#[test]
fn horizontal_scroll_is_a_noop_in_unified() {
    let dir = TempDir::new().unwrap();
    wide_line_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    app.update(Action::OpenPanel);
    app.update(Action::OpenPanel); // fullscreen, unified, file focus
    app.update(Action::ToggleFocus); // focus the diff (still unified)
    app.update(Action::ScrollDiffRight);

    assert_eq!(
        app.panel().unwrap().diff_hscroll,
        0,
        "unified diffs do not scroll horizontally"
    );
}
