use std::path::Path;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use git2::{Oid, Repository, RepositoryInitOptions, Signature, Time};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use tempfile::TempDir;

use ogma::app::{Action, App};
use ogma::event::InputMap;
use ogma::ui::render;

const NOW: i64 = 100_000;

/// Builds a small repository with a fork so the graph, badges and status bar
/// all have something real to render:
///
///   main    ●  Add feature base   (HEAD)
///   feature │● Work on feature
///           ●  Initial commit      (fork point)
fn fixture_repo(dir: &Path) {
    let mut opts = RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(dir, &opts).unwrap();

    let ada = Signature::new("Ada Lovelace", "ada@example.com", &Time::new(1000, 0)).unwrap();
    let root = commit(
        &repo,
        &ada,
        None,
        "README",
        "hello\n",
        "Initial commit",
        &[],
        true,
    );

    let ada_later = Signature::new("Ada Lovelace", "ada@example.com", &Time::new(1002, 0)).unwrap();
    commit(
        &repo,
        &ada_later,
        Some(root),
        "main.rs",
        "fn main() {}\n",
        "Add feature base",
        &[root],
        true,
    );

    let grace = Signature::new("Grace Hopper", "grace@example.com", &Time::new(1001, 0)).unwrap();
    let feature = commit(
        &repo,
        &grace,
        Some(root),
        "feature.rs",
        "// wip\n",
        "Work on feature",
        &[root],
        false,
    );
    repo.branch("feature", &repo.find_commit(feature).unwrap(), true)
        .unwrap();
}

fn linear_repo(dir: &Path, count: usize) {
    let mut opts = RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(dir, &opts).unwrap();

    let mut parent: Option<Oid> = None;
    for index in 1..=count {
        let signature = Signature::new(
            "Ada Lovelace",
            "ada@example.com",
            &Time::new(1000 + index as i64, 0),
        )
        .unwrap();
        let parents: Vec<Oid> = parent.into_iter().collect();
        parent = Some(commit(
            &repo,
            &signature,
            parent,
            "log.txt",
            &format!("line {index}\n"),
            &format!("commit {index}"),
            &parents,
            true,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn commit(
    repo: &Repository,
    signature: &Signature,
    base: Option<Oid>,
    path: &str,
    content: &str,
    message: &str,
    parents: &[Oid],
    update_head: bool,
) -> Oid {
    let blob = repo.blob(content.as_bytes()).unwrap();
    let base_tree = base.map(|oid| repo.find_commit(oid).unwrap().tree().unwrap());
    let mut builder = repo.treebuilder(base_tree.as_ref()).unwrap();
    builder.insert(path, blob, 0o100644).unwrap();
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();

    let parent_commits: Vec<_> = parents
        .iter()
        .map(|oid| repo.find_commit(*oid).unwrap())
        .collect();
    let parent_refs: Vec<_> = parent_commits.iter().collect();

    let target = if update_head { Some("HEAD") } else { None };
    repo.commit(target, signature, signature, message, &tree, &parent_refs)
        .unwrap()
}

fn app_for(dir: &TempDir) -> App {
    App::open(dir.path()).unwrap()
}

fn draw(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, app, NOW)).unwrap();
    terminal.backend().buffer().clone()
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }
}

fn press(app: &mut App, input: &mut InputMap, code: KeyCode) {
    if let Some(action) = input.on_key(key(code)) {
        app.update(action);
    }
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

fn marker_row(buffer: &Buffer) -> Option<u16> {
    let area = buffer.area();
    (0..area.height).find(|&y| buffer.cell((0, y)).unwrap().symbol() == "❯")
}

#[test]
fn initial_frame_shows_graph_badges_and_status() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = app_for(&dir);

    let screen = dump(&draw(&mut app, 80, 20));

    assert!(screen.contains("Initial commit"), "{screen}");
    assert!(screen.contains("Add feature base"), "{screen}");
    assert!(screen.contains("Work on feature"), "{screen}");
    assert!(screen.contains("main"), "branch badge missing:\n{screen}");
    assert!(
        screen.contains("feature"),
        "feature badge missing:\n{screen}"
    );
    assert!(screen.contains("Grace Hopper"), "author missing:\n{screen}");
    assert!(screen.contains("1/3"), "status position missing:\n{screen}");
    assert!(screen.contains('●'), "graph node glyph missing:\n{screen}");
}

#[test]
fn selection_starts_on_the_first_row() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = app_for(&dir);

    let buffer = draw(&mut app, 80, 20);

    assert_eq!(marker_row(&buffer), Some(0));
}

#[test]
fn j_moves_the_selection_marker_down() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = app_for(&dir);
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('j'));
    let buffer = draw(&mut app, 80, 20);

    assert_eq!(marker_row(&buffer), Some(1));
}

#[test]
fn capital_g_jumps_to_the_last_commit() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = app_for(&dir);
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('G'));
    let buffer = draw(&mut app, 80, 20);

    assert_eq!(marker_row(&buffer), Some(2));
    assert_eq!(app.selected(), 2);
}

#[test]
fn gg_prefix_jumps_back_to_the_first_commit() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = app_for(&dir);
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('G'));
    press(&mut app, &mut input, KeyCode::Char('g'));
    press(&mut app, &mut input, KeyCode::Char('g'));
    let buffer = draw(&mut app, 80, 20);

    assert_eq!(marker_row(&buffer), Some(0));
}

#[test]
fn enter_opens_the_detail_panel_after_the_slide_animation() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = app_for(&dir);
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Enter);
    for _ in 0..20 {
        app.update(Action::Tick(Duration::from_millis(16)));
    }
    let screen = dump(&draw(&mut app, 80, 20));

    assert!(screen.contains("Commit"), "panel title missing:\n{screen}");
    assert!(
        screen.contains("Author"),
        "panel author label missing:\n{screen}"
    );
    assert!(
        screen.contains("Files"),
        "panel files section missing:\n{screen}"
    );
    assert!(
        screen.contains("main.rs"),
        "changed file missing:\n{screen}"
    );
}

#[test]
fn esc_closes_the_detail_panel() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = app_for(&dir);
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Enter);
    for _ in 0..20 {
        app.update(Action::Tick(Duration::from_millis(16)));
    }
    press(&mut app, &mut input, KeyCode::Esc);
    for _ in 0..20 {
        app.update(Action::Tick(Duration::from_millis(16)));
    }

    assert!(app.panel().is_none());
}

#[test]
fn head_badge_marks_the_checked_out_commit() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = app_for(&dir);

    let screen = dump(&draw(&mut app, 80, 20));

    assert!(screen.contains("HEAD"), "HEAD badge missing:\n{screen}");
}

#[test]
fn capital_g_loads_past_the_first_page_to_reach_the_root() {
    let dir = TempDir::new().unwrap();
    linear_repo(dir.path(), 8);
    let mut app = App::open_with_load_page(dir.path(), 3).unwrap();
    let mut input = InputMap::default();

    assert_eq!(app.commits().len(), 3);

    press(&mut app, &mut input, KeyCode::Char('G'));
    let screen = dump(&draw(&mut app, 80, 20));

    assert_eq!(app.commits().len(), 8);
    assert_eq!(app.selected(), 7);
    assert!(screen.contains("commit 1"), "root not reached:\n{screen}");
}

#[test]
fn wheel_scroll_moves_the_viewport_but_not_the_selection() {
    let dir = TempDir::new().unwrap();
    linear_repo(dir.path(), 8);
    let mut app = app_for(&dir);

    draw(&mut app, 80, 6);
    app.update(Action::ScrollDown);

    assert_eq!(app.selected(), 0);
    assert!(app.offset() > 0, "viewport did not scroll");
}

#[test]
fn question_mark_toggles_the_help_overlay() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = app_for(&dir);
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('?'));
    let screen = dump(&draw(&mut app, 80, 20));

    assert!(screen.contains("Help"), "help title missing:\n{screen}");
    assert!(screen.contains("quit"), "help binding missing:\n{screen}");
}
