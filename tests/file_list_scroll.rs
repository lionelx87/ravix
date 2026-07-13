use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
use git2::{Oid, Repository, RepositoryInitOptions, Signature, Time};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use tempfile::TempDir;

use ravix::app::{Action, App, InputContext};
use ravix::event::InputMap;
use ravix::ui::render;

const NOW: i64 = 100_000;

fn commit(repo: &Repository, base: Option<Oid>, files: &[(String, String)], message: &str) -> Oid {
    let signature = Signature::new("Dev", "dev@example.com", &Time::new(1000, 0)).unwrap();
    let base_tree = base.map(|oid| repo.find_commit(oid).unwrap().tree().unwrap());
    let mut builder = repo.treebuilder(base_tree.as_ref()).unwrap();
    for (path, body) in files {
        let blob = repo.blob(body.as_bytes()).unwrap();
        builder.insert(path, blob, 0o100644).unwrap();
    }
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let parents: Vec<_> = base
        .into_iter()
        .map(|oid| repo.find_commit(oid).unwrap())
        .collect();
    let refs: Vec<_> = parents.iter().collect();
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &refs)
        .unwrap()
}

fn many_file_repo(dir: &TempDir) {
    let mut opts = RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(dir.path(), &opts).unwrap();
    let root = commit(&repo, None, &[("seed.txt".into(), "seed\n".into())], "seed");
    let files: Vec<(String, String)> = (0..40)
        .map(|n| (format!("file_{n:02}.txt"), format!("body {n}\n")))
        .collect();
    commit(&repo, Some(root), &files, "add many files");
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

#[test]
fn navigating_down_scrolls_the_file_list_to_keep_the_selection_visible() {
    let dir = TempDir::new().unwrap();
    many_file_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    app.update(Action::OpenPanel);
    app.update(Action::OpenPanel); // fullscreen, file list focused

    let top = dump(&draw(&mut app, 160, 20));
    assert!(
        !top.contains("file_30"),
        "a file far down the list is below the fold at first:\n{top}"
    );

    for _ in 0..30 {
        app.update(Action::SelectNext);
    }
    let scrolled = dump(&draw(&mut app, 160, 20));
    assert!(
        app.panel().unwrap().files_scroll > 0,
        "the file list scrolled to follow the cursor"
    );
    assert!(
        scrolled.contains("file_30"),
        "the selected file is scrolled into view:\n{scrolled}"
    );
}

fn wheel(kind: MouseEventKind, column: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row: 5,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn wheel_over_the_file_list_moves_the_file_cursor() {
    let mut input = InputMap::default();
    let area = Rect::new(0, 0, 160, 40);

    assert_eq!(
        input.on_mouse(
            wheel(MouseEventKind::ScrollDown, 5),
            area,
            InputContext::CommitDiff
        ),
        Some(Action::ScrollFilesDown),
        "the wheel over the left file-list pane scrolls the files"
    );
    assert_eq!(
        input.on_mouse(
            wheel(MouseEventKind::ScrollDown, 100),
            area,
            InputContext::CommitDiff
        ),
        Some(Action::ScrollDown),
        "the wheel over the diff pane scrolls the diff"
    );
    assert_eq!(
        input.on_mouse(
            wheel(MouseEventKind::ScrollDown, 5),
            area,
            InputContext::Graph
        ),
        Some(Action::ScrollDown),
        "outside the fullscreen commit diff the wheel keeps its graph behavior"
    );
}

#[test]
fn scroll_files_action_advances_the_file_cursor() {
    let dir = TempDir::new().unwrap();
    many_file_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    app.update(Action::OpenPanel);
    app.update(Action::OpenPanel);
    let before = app.panel().unwrap().file;
    app.update(Action::ScrollFilesDown);
    assert!(
        app.panel().unwrap().file > before,
        "ScrollFilesDown moves the file cursor forward"
    );
}
