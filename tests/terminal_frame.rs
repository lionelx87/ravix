use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use git2::{Oid, Repository, RepositoryInitOptions, Signature, Time};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
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
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
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
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
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
    if let Some(action) = input.on_key(key(code), app.input_context()) {
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

fn dirty_repo(dir: &Path) {
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };

    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "demo@ogma.dev"]);
    run(&["config", "user.name", "Demo"]);

    let base: String = (1..=12).map(|n| format!("line {n}\n")).collect();
    std::fs::write(dir.join("app.txt"), &base).unwrap();
    std::fs::write(dir.join("readme.md"), "alpha\nbeta\ngamma\n").unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "initial commit"]);

    let edited = base
        .replace("line 1\n", "LINE ONE\n")
        .replace("line 12\n", "LINE TWELVE\n");
    std::fs::write(dir.join("app.txt"), &edited).unwrap();
    std::fs::write(dir.join("readme.md"), "alpha\nBETA\ngamma\ndelta\n").unwrap();
    run(&["add", "readme.md"]);
    std::fs::write(dir.join("notes.txt"), "todo\n").unwrap();
}

fn settle(app: &mut App) {
    for _ in 0..24 {
        app.update(Action::Tick(Duration::from_millis(16)));
    }
}

fn open_working(app: &mut App, input: &mut InputMap) {
    press(app, input, KeyCode::Char('k'));
    press(app, input, KeyCode::Enter);
    settle(app);
}

#[test]
fn wip_node_appears_when_the_tree_is_dirty() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();

    let screen = dump(&draw(&mut app, 100, 24));

    assert!(
        screen.contains("Uncommitted changes"),
        "WIP node missing:\n{screen}"
    );
    assert!(screen.contains("staged"), "WIP counts missing:\n{screen}");
}

#[test]
fn opening_the_wip_panel_lists_the_changed_files() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    let screen = dump(&draw(&mut app, 100, 28));

    assert!(screen.contains("Unstaged"), "section missing:\n{screen}");
    assert!(screen.contains("Staged"), "section missing:\n{screen}");
    assert!(screen.contains("Untracked"), "section missing:\n{screen}");
    assert!(screen.contains("app.txt"), "file missing:\n{screen}");
    assert!(screen.contains("readme.md"), "file missing:\n{screen}");
    assert!(screen.contains("notes.txt"), "file missing:\n{screen}");
}

#[test]
fn enter_expands_the_panel_to_a_fullscreen_two_pane() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Enter);
    let screen = dump(&draw(&mut app, 100, 28));

    assert!(
        screen.contains("Working directory"),
        "fullscreen pane missing:\n{screen}"
    );
}

#[test]
fn space_stages_the_focused_file() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    let before = dump(&draw(&mut app, 100, 28));
    assert!(
        before.contains("Unstaged"),
        "expected unstaged section:\n{before}"
    );

    press(&mut app, &mut input, KeyCode::Char(' '));
    let after = dump(&draw(&mut app, 100, 28));

    assert!(
        !after.contains("Unstaged"),
        "unstaged section should be gone after staging the only unstaged file:\n{after}"
    );
    assert!(after.contains("Staged"), "staged section missing:\n{after}");
    assert!(after.contains("app.txt"), "app.txt missing:\n{after}");
}

#[test]
fn space_on_a_hunk_leaves_the_file_partially_staged() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Tab);
    press(&mut app, &mut input, KeyCode::Char(' '));
    let after = dump(&draw(&mut app, 100, 28));

    assert!(
        after.contains("Unstaged") && after.contains("Staged"),
        "both sections should remain for a partially staged file:\n{after}"
    );
    assert!(
        after.matches("app.txt").count() >= 2,
        "app.txt should appear in both the unstaged and staged sections:\n{after}"
    );
}

#[test]
fn esc_closes_the_commit_editor_but_keeps_the_working_view() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('c'));
    assert!(app.commit_editor().is_some(), "commit editor did not open");

    press(&mut app, &mut input, KeyCode::Esc);

    assert!(
        app.commit_editor().is_none(),
        "Esc should close the commit editor"
    );
    assert!(
        app.working().is_some(),
        "Esc should not close the working view underneath"
    );
}

#[test]
fn committing_from_the_editor_creates_a_commit() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('c'));
    for character in "stage readme".chars() {
        press(&mut app, &mut input, KeyCode::Char(character));
    }
    press(&mut app, &mut input, KeyCode::Enter);
    let after = dump(&draw(&mut app, 100, 28));

    assert!(
        after.contains("stage readme"),
        "the new commit should show in the graph:\n{after}"
    );
    assert_eq!(app.commits()[0].summary, "stage readme");
    assert!(
        !app.status().staged.iter().any(|f| f.path == "readme.md"),
        "readme.md should have been committed: {:?}",
        app.status()
    );
}

fn code_repo(dir: &Path) {
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };

    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "demo@ogma.dev"]);
    run(&["config", "user.name", "Demo"]);

    std::fs::write(
        dir.join("main.rs"),
        "fn main() {\n    let value = 1;\n    println!(\"{}\", value);\n}\n",
    )
    .unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "initial commit"]);

    std::fs::write(
        dir.join("main.rs"),
        "fn main() {\n    let result = 2;\n    println!(\"{}\", value);\n}\n",
    )
    .unwrap();
}

fn find_row(buffer: &Buffer, needle: &str) -> Option<u16> {
    let area = buffer.area();
    (0..area.height).find(|&y| {
        let row: String = (0..area.width)
            .map(|x| buffer.cell((x, y)).unwrap().symbol())
            .collect();
        row.contains(needle)
    })
}

fn row_colors(buffer: &Buffer, y: u16, background: bool) -> HashSet<Color> {
    let area = buffer.area();
    (0..area.width)
        .map(|x| {
            let cell = buffer.cell((x, y)).unwrap();
            if background { cell.bg } else { cell.fg }
        })
        .filter(|color| *color != Color::Reset)
        .collect()
}

#[test]
fn diff_view_syntax_highlights_and_marks_intraline_changes() {
    let dir = TempDir::new().unwrap();
    code_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    let buffer = draw(&mut app, 120, 30);

    let context_row = find_row(&buffer, "fn main").expect("context line should render");
    assert!(
        row_colors(&buffer, context_row, false).len() >= 2,
        "the context line should carry more than one syntax color"
    );

    let added_row = find_row(&buffer, "result").expect("added line should render");
    assert!(
        row_colors(&buffer, added_row, true).len() >= 2,
        "the changed word should carry an intra-line emphasis background"
    );
}

#[test]
fn discarding_a_hunk_reverts_only_that_hunk_in_the_working_tree() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Tab);
    press(&mut app, &mut input, KeyCode::Char('d'));
    assert!(
        app.confirm().is_some(),
        "discard should ask for confirmation"
    );
    press(&mut app, &mut input, KeyCode::Char('y'));

    let content = std::fs::read_to_string(dir.path().join("app.txt")).unwrap();
    assert!(
        content.contains("line 1\n"),
        "the discarded hunk (line 1) should be restored:\n{content}"
    );
    assert!(
        content.contains("LINE TWELVE"),
        "the untouched hunk (line 12) should remain:\n{content}"
    );
}
