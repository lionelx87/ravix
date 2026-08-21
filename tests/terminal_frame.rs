use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use git2::{Oid, Repository, RepositoryInitOptions, Signature, Time};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use tempfile::TempDir;

use ravix::app::{Action, App, InputContext};
use ravix::event::InputMap;
use ravix::ui::render;

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

fn three_branch_repo(dir: &Path) {
    let mut opts = RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(dir, &opts).unwrap();

    let sig = |t| Signature::new("Dev", "dev@example.com", &Time::new(t, 0)).unwrap();
    let root = commit(
        &repo,
        &sig(1000),
        None,
        "README",
        "root\n",
        "Root",
        &[],
        true,
    );
    commit(
        &repo,
        &sig(1003),
        Some(root),
        "main.rs",
        "// main\n",
        "Main work",
        &[root],
        true,
    );
    let alpha = commit(
        &repo,
        &sig(1002),
        Some(root),
        "alpha.rs",
        "// alpha\n",
        "Alpha work",
        &[root],
        false,
    );
    repo.branch("alpha", &repo.find_commit(alpha).unwrap(), true)
        .unwrap();
    let beta = commit(
        &repo,
        &sig(1001),
        Some(root),
        "beta.rs",
        "// beta\n",
        "Beta work",
        &[root],
        false,
    );
    repo.branch("beta", &repo.find_commit(beta).unwrap(), true)
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
    let panel_side = area.width / 2;
    (0..area.height)
        .find(|&y| (0..panel_side).any(|x| buffer.cell((x, y)).unwrap().symbol() == "❯"))
}

fn panel_marker_row(buffer: &Buffer) -> Option<u16> {
    let area = buffer.area();
    let panel_side = area.width / 2;
    (0..area.height)
        .find(|&y| (panel_side..area.width).any(|x| buffer.cell((x, y)).unwrap().symbol() == "❯"))
}

fn panel_marker_line(buffer: &Buffer) -> Option<String> {
    let area = buffer.area();
    let panel_side = area.width / 2;
    let row = panel_marker_row(buffer)?;
    let mut line = String::new();
    for x in panel_side..area.width {
        line.push_str(buffer.cell((x, row)).unwrap().symbol());
    }
    Some(line)
}

fn status_code_fg(buffer: &Buffer, path: &str) -> Color {
    let area = buffer.area();
    let panel_side = area.width / 2;
    for y in 0..area.height {
        let mut line = String::new();
        for x in panel_side..area.width {
            line.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        if line.contains(path) {
            for x in panel_side..area.width {
                let symbol = buffer.cell((x, y)).unwrap().symbol();
                if symbol.chars().count() == 1
                    && symbol.chars().next().unwrap().is_ascii_uppercase()
                {
                    return buffer.cell((x, y)).unwrap().fg;
                }
            }
        }
    }
    panic!("no status code found on the row for {path}");
}

fn node_fg(buffer: &Buffer, summary: &str) -> Color {
    let area = buffer.area();
    for y in 0..area.height {
        let mut line = String::new();
        for x in 0..area.width {
            line.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        if line.contains(summary) {
            for x in 0..area.width {
                if buffer.cell((x, y)).unwrap().symbol() == "●" {
                    return buffer.cell((x, y)).unwrap().fg;
                }
            }
        }
    }
    panic!("no node glyph found on the row for {summary}");
}

fn panel_region(buffer: &Buffer) -> String {
    let area = buffer.area();
    let panel_side = area.width / 2;
    let mut out = String::new();
    for y in 0..area.height {
        for x in panel_side..area.width {
            out.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        out.push('\n');
    }
    out
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

    assert_eq!(marker_row(&buffer), Some(1));
}

#[test]
fn j_moves_the_selection_marker_down() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = app_for(&dir);
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('j'));
    let buffer = draw(&mut app, 80, 20);

    assert_eq!(marker_row(&buffer), Some(2));
}

#[test]
fn capital_g_jumps_to_the_last_commit() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = app_for(&dir);
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('G'));
    let buffer = draw(&mut app, 80, 20);

    assert_eq!(marker_row(&buffer), Some(3));
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

    assert_eq!(marker_row(&buffer), Some(1));
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
fn the_checked_out_commit_row_is_highlighted_in_teal() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = app_for(&dir);

    let buffer = draw(&mut app, 80, 20);
    let screen = dump(&buffer);

    let area = buffer.area();
    let highlighted = (0..area.height).any(|y| {
        (0..area.width).any(|x| buffer.cell((x, y)).unwrap().bg == Color::Rgb(13, 74, 80))
    });
    assert!(
        highlighted,
        "the checked-out commit row carries the teal highlight:\n{screen}"
    );
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
fn help_is_context_aware() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('?'));
    let graph_help = dump(&draw(&mut app, 100, 24));
    assert!(
        graph_help.contains("Help — Graph"),
        "the graph help is titled Graph:\n{graph_help}"
    );
    assert!(
        !graph_help.contains("solo"),
        "the graph help does not list branch-panel-only keys:\n{graph_help}"
    );
    assert!(graph_help.contains("quit"), "the universal footer shows");
    press(&mut app, &mut input, KeyCode::Char('?'));

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('?'));
    let branch_help = dump(&draw(&mut app, 100, 24));
    assert!(
        branch_help.contains("Help — Branches"),
        "the branch panel help is titled Branches:\n{branch_help}"
    );
    assert!(
        branch_help.contains("solo"),
        "the branch help lists the branch-panel keys:\n{branch_help}"
    );
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
    assert!(
        screen.contains("(detached HEAD if no branch)"),
        "the overlay is wide enough for its longest description:\n{screen}"
    );
}

fn dirty_repo(dir: &Path) {
    let run = |args: &[&str]| git_run(dir, args);

    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "demo@ravix.dev"]);
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

fn submodule_repo(dir: &Path) -> std::path::PathBuf {
    let git = git_run;

    let origin = dir.join("sub-origin");
    std::fs::create_dir(&origin).unwrap();
    git(&origin, &["init", "-q", "-b", "main"]);
    git(&origin, &["config", "user.email", "d@e.com"]);
    git(&origin, &["config", "user.name", "Dev"]);
    std::fs::write(origin.join("lib.rs"), "// lib\n").unwrap();
    git(&origin, &["add", "-A"]);
    git(&origin, &["commit", "-q", "-m", "Submodule work"]);

    let parent = dir.join("parent");
    std::fs::create_dir(&parent).unwrap();
    git(&parent, &["init", "-q", "-b", "main"]);
    git(&parent, &["config", "user.email", "d@e.com"]);
    git(&parent, &["config", "user.name", "Dev"]);
    std::fs::write(parent.join("main.rs"), "// main\n").unwrap();
    git(&parent, &["add", "-A"]);
    git(&parent, &["commit", "-q", "-m", "Parent work"]);
    git(
        &parent,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            origin.to_str().unwrap(),
            "vendored",
        ],
    );
    git(&parent, &["commit", "-q", "-m", "Add submodule"]);
    parent
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
fn focusing_an_untracked_file_shows_its_content_in_the_diff_pane() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Enter);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('j'));
    let screen = dump(&draw(&mut app, 100, 28));

    assert!(
        screen.contains("todo"),
        "the untracked file's content renders as an all-added diff:\n{screen}"
    );
    assert!(
        !screen.contains("No textual diff"),
        "the empty-diff fallback should not show for a readable untracked file:\n{screen}"
    );
}

fn any_row_has_both(screen: &str, left: &str, right: &str) -> bool {
    screen.lines().any(|line| {
        line.find(left)
            .zip(line.rfind(right))
            .is_some_and(|(l, r)| l < r)
    })
}

#[test]
fn a_long_left_line_is_truncated_so_the_right_column_stays_visible() {
    let dir = TempDir::new().unwrap();
    let run = |args: &[&str]| git_run(dir.path(), args);
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "demo@ravix.dev"]);
    run(&["config", "user.name", "Demo"]);
    std::fs::write(dir.path().join("app.txt"), format!("{}\n", "x".repeat(120))).unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "long line"]);
    std::fs::write(dir.path().join("app.txt"), "SENTINEL\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Enter);
    press(&mut app, &mut input, KeyCode::Char('v'));

    let screen = dump(&draw(&mut app, 100, 28));
    assert!(
        screen.contains("SENTINEL"),
        "the long old line is clipped to its column so the new line stays on screen:\n{screen}"
    );
}

#[test]
fn v_toggles_the_fullscreen_diff_to_side_by_side() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Enter);
    let unified = dump(&draw(&mut app, 100, 28));
    assert!(
        !any_row_has_both(&unified, "line 1", "LINE ONE"),
        "unified keeps old and new on separate rows:\n{unified}"
    );

    press(&mut app, &mut input, KeyCode::Char('v'));
    let split = dump(&draw(&mut app, 100, 28));
    assert!(
        any_row_has_both(&split, "line 1", "LINE ONE"),
        "side-by-side puts old (left) and new (right) on the same row:\n{split}"
    );

    press(&mut app, &mut input, KeyCode::Char('v'));
    let back = dump(&draw(&mut app, 100, 28));
    assert!(
        !any_row_has_both(&back, "line 1", "LINE ONE"),
        "v again returns to unified:\n{back}"
    );
}

fn two_commit_repo(dir: &Path) {
    git_run(dir, &["init", "-q", "-b", "main"]);
    git_run(dir, &["config", "user.email", "d@e.com"]);
    git_run(dir, &["config", "user.name", "Dev"]);
    std::fs::write(dir.join("app.txt"), "alpha\nbeta\n").unwrap();
    git_run(dir, &["add", "-A"]);
    git_run(dir, &["commit", "-q", "-m", "First commit"]);
    std::fs::write(dir.join("app.txt"), "alpha\nBETACHANGED\n").unwrap();
    git_run(dir, &["add", "-A"]);
    git_run(dir, &["commit", "-q", "-m", "Second commit"]);
}

#[test]
fn the_commit_panel_colours_file_status_codes_by_kind() {
    let dir = TempDir::new().unwrap();
    git_run(dir.path(), &["init", "-q", "-b", "main"]);
    git_run(dir.path(), &["config", "user.email", "d@e.com"]);
    git_run(dir.path(), &["config", "user.name", "Dev"]);
    std::fs::write(dir.path().join("keep.txt"), "one\n").unwrap();
    git_run(dir.path(), &["add", "-A"]);
    git_run(dir.path(), &["commit", "-q", "-m", "first"]);
    std::fs::write(dir.path().join("keep.txt"), "two\n").unwrap();
    std::fs::write(dir.path().join("new.txt"), "added\n").unwrap();
    git_run(dir.path(), &["add", "-A"]);
    git_run(dir.path(), &["commit", "-q", "-m", "second"]);

    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    let buffer = draw(&mut app, 100, 28);

    let modified = status_code_fg(&buffer, "keep.txt");
    let added = status_code_fg(&buffer, "new.txt");
    assert_ne!(
        modified, added,
        "the M and A status codes are coloured differently by change kind"
    );
}

#[test]
fn enter_expands_a_commit_to_a_fullscreen_line_level_diff() {
    let dir = TempDir::new().unwrap();
    two_commit_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    let unified = dump(&draw(&mut app, 100, 28));
    assert!(
        unified.contains("BETACHANGED"),
        "the fullscreen commit diff shows the added line:\n{unified}"
    );

    press(&mut app, &mut input, KeyCode::Char('v'));
    let split = dump(&draw(&mut app, 100, 28));
    assert!(
        any_row_has_both(&split, "beta", "BETACHANGED"),
        "side-by-side puts old and new on the same row:\n{split}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);
    let collapsed = dump(&draw(&mut app, 100, 28));
    assert!(
        !collapsed.contains("BETACHANGED"),
        "Esc collapses the fullscreen diff:\n{collapsed}"
    );
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

#[test]
fn the_open_branch_list_refreshes_on_reload_after_an_external_ref_change() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    let before = panel_region(&draw(&mut app, 80, 20));
    assert!(
        !before.contains("hotfix"),
        "the externally-created branch is not listed yet:\n{before}"
    );

    git_run(dir.path(), &["branch", "hotfix"]);
    app.update(Action::Reload);
    settle(&mut app);

    let after = panel_region(&draw(&mut app, 80, 20));
    assert!(
        after.contains("hotfix"),
        "Reload refreshes the open branch list with the new ref:\n{after}"
    );
}

#[test]
fn b_opens_the_branch_list_marking_the_current_branch() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    let screen = dump(&draw(&mut app, 80, 20));

    assert!(
        screen.contains("Branches"),
        "branch panel title missing:\n{screen}"
    );
    assert!(screen.contains("main"), "branch missing:\n{screen}");
    assert!(screen.contains("feature"), "branch missing:\n{screen}");
    assert!(
        screen.contains('●'),
        "the current branch should carry a HEAD marker:\n{screen}"
    );
    assert!(
        screen.contains("M join"),
        "the panel lists its action keys, including join:\n{screen}"
    );
    assert!(
        screen.contains("o solo"),
        "the panel lists its visibility keys:\n{screen}"
    );
}

#[test]
fn a_branch_lines_color_is_distinct_and_stable_when_another_branch_is_hidden() {
    let dir = TempDir::new().unwrap();
    three_branch_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    let beta_before = node_fg(&draw(&mut app, 100, 20), "Beta work");
    let main_color = node_fg(&draw(&mut app, 100, 20), "Main work");
    assert_ne!(
        beta_before, main_color,
        "distinct branches get distinct node colors"
    );

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);

    let beta_after = node_fg(&draw(&mut app, 100, 20), "Beta work");
    assert_eq!(
        beta_before, beta_after,
        "beta keeps its color when alpha is hidden and columns shift"
    );
}

#[test]
fn activating_a_focus_set_recomposes_the_graph_to_it() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);

    press(&mut app, &mut input, KeyCode::Char('F'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('n'));
    for ch in "hide-feature".chars() {
        press(&mut app, &mut input, KeyCode::Char(ch));
    }
    press(&mut app, &mut input, KeyCode::Enter);
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);
    let shown = dump(&draw(&mut app, 80, 20));
    assert!(
        shown.contains("Work on feature"),
        "feature is visible again before activating the set:\n{shown}"
    );

    press(&mut app, &mut input, KeyCode::Char('F'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    let restored = dump(&draw(&mut app, 80, 20));
    assert!(
        !restored.contains("Work on feature"),
        "activating the saved set hides feature again:\n{restored}"
    );
}

#[test]
fn j_moves_the_focus_panel_selection_marker() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('F'));
    settle(&mut app);
    for name in ["aaa", "bbb"] {
        press(&mut app, &mut input, KeyCode::Char('n'));
        for ch in name.chars() {
            press(&mut app, &mut input, KeyCode::Char(ch));
        }
        press(&mut app, &mut input, KeyCode::Enter);
    }

    let before = panel_marker_row(&draw(&mut app, 80, 20)).expect("marker on open");
    press(&mut app, &mut input, KeyCode::Char('j'));
    let after = panel_marker_row(&draw(&mut app, 80, 20)).expect("marker after move");

    assert_eq!(
        after,
        before + 1,
        "j moves the focus panel's own selection, not the graph underneath"
    );
}

#[test]
fn entering_a_submodule_preserves_the_celebrations_intensity() {
    let dir = TempDir::new().unwrap();
    let parent = submodule_repo(dir.path());
    let mut app = App::open(&parent).unwrap();
    let mut input = InputMap::default();

    app.update(Action::CycleCelebrations);

    press(&mut app, &mut input, KeyCode::Char('>'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);

    app.update(Action::CycleCelebrations);
    let screen = dump(&draw(&mut app, 100, 24));
    assert!(
        screen.contains("Celebrations: off"),
        "the celebrations dial (a user preference) survives entering a submodule:\n{screen}"
    );
}

#[test]
fn entering_a_submodule_switches_the_graph_and_shows_a_breadcrumb() {
    let dir = TempDir::new().unwrap();
    let parent = submodule_repo(dir.path());
    let mut app = App::open(&parent).unwrap();
    let mut input = InputMap::default();

    let before = dump(&draw(&mut app, 120, 24));
    assert!(
        before.contains("Parent work"),
        "parent graph shown:\n{before}"
    );

    press(&mut app, &mut input, KeyCode::Char('>'));
    settle(&mut app);
    let panel = panel_region(&draw(&mut app, 120, 24));
    assert!(
        panel.contains("vendored"),
        "the submodule is listed in the panel:\n{panel}"
    );

    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    let inside = dump(&draw(&mut app, 120, 24));
    assert!(
        inside.contains("Submodule work"),
        "the graph switches to the submodule:\n{inside}"
    );
    assert!(
        !inside.contains("Parent work"),
        "the parent graph is gone:\n{inside}"
    );
    assert!(
        inside.contains("vendored"),
        "the breadcrumb shows the submodule:\n{inside}"
    );

    press(&mut app, &mut input, KeyCode::Char('<'));
    settle(&mut app);
    let back = dump(&draw(&mut app, 120, 24));
    assert!(
        back.contains("Parent work"),
        "exiting returns to the parent:\n{back}"
    );
}

#[test]
fn a_saved_focus_set_survives_a_relaunch() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('F'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('n'));
    for ch in "myset".chars() {
        press(&mut app, &mut input, KeyCode::Char(ch));
    }
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);

    let mut relaunched = App::open(dir.path()).unwrap();
    let mut input2 = InputMap::default();
    press(&mut relaunched, &mut input2, KeyCode::Char('F'));
    settle(&mut relaunched);
    let panel = panel_region(&draw(&mut relaunched, 80, 20));
    assert!(
        panel.contains("myset"),
        "the saved focus set is written to disk and listed in the focus panel after relaunch:\n{panel}"
    );
}

#[test]
fn hiding_a_branch_recomposes_the_graph_without_its_exclusive_commits() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    let before = dump(&draw(&mut app, 80, 20));
    assert!(
        before.contains("Work on feature"),
        "the feature branch's exclusive commit is present initially:\n{before}"
    );

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);

    let after = dump(&draw(&mut app, 80, 20));
    assert!(
        !after.contains("Work on feature"),
        "hiding feature drops its exclusive commit from the graph:\n{after}"
    );
    assert!(
        after.contains("Initial commit"),
        "the shared fork-point commit remains:\n{after}"
    );
    assert!(
        after.contains("Add feature base"),
        "main's own commit remains:\n{after}"
    );
}

#[test]
fn slash_filters_the_branch_list_to_matches() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    let before = panel_region(&draw(&mut app, 80, 20));
    assert!(
        before.contains("main"),
        "main is listed before filtering:\n{before}"
    );

    press(&mut app, &mut input, KeyCode::Char('/'));
    for ch in "feat".chars() {
        press(&mut app, &mut input, KeyCode::Char(ch));
    }

    let after = panel_region(&draw(&mut app, 80, 20));
    assert!(
        after.contains("feature"),
        "the matching branch stays listed:\n{after}"
    );
    assert!(
        !after.contains("main"),
        "the non-matching branch is filtered out of the panel:\n{after}"
    );
}

#[test]
fn a_submitted_filter_keeps_branch_actions_available() {
    let dir = TempDir::new().unwrap();
    three_branch_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('/'));
    for ch in "alp".chars() {
        press(&mut app, &mut input, KeyCode::Char(ch));
    }
    press(&mut app, &mut input, KeyCode::Enter);
    press(&mut app, &mut input, KeyCode::Char('o'));
    press(&mut app, &mut input, KeyCode::Esc);
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);

    let after = dump(&draw(&mut app, 100, 24));
    assert!(
        after.contains("Alpha work"),
        "the soloed filtered branch keeps its commit:\n{after}"
    );
    assert!(
        !after.contains("Beta work"),
        "solo applied from the filtered list hides the other branch:\n{after}"
    );
}

#[test]
fn arrow_keys_move_the_selection_while_typing_a_filter() {
    let dir = TempDir::new().unwrap();
    three_branch_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('/'));
    press(&mut app, &mut input, KeyCode::Char('a'));
    let before = panel_marker_row(&draw(&mut app, 80, 20)).expect("marker while editing");

    press(&mut app, &mut input, KeyCode::Down);
    let after = panel_marker_row(&draw(&mut app, 80, 20)).expect("marker after Down");

    assert_eq!(
        after,
        before + 1,
        "Down moves the selection through the matches while the filter is being typed"
    );
}

#[test]
fn typing_a_filter_selects_the_best_match() {
    let dir = TempDir::new().unwrap();
    three_branch_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    let top = panel_marker_row(&draw(&mut app, 80, 20)).expect("marker on the first branch");

    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('j'));
    let moved = panel_marker_row(&draw(&mut app, 80, 20)).expect("marker after moving down");
    assert!(
        moved > top,
        "the selection moved away from the first row before filtering"
    );

    press(&mut app, &mut input, KeyCode::Char('/'));
    press(&mut app, &mut input, KeyCode::Char('a'));
    let frame = draw(&mut app, 80, 20);
    let filtered = panel_marker_row(&frame).expect("marker while filtering");
    let line = panel_marker_line(&frame).expect("selected row while filtering");

    assert_eq!(
        filtered, top,
        "typing a filter snaps the selection back to the top of the matches"
    );
    assert!(
        line.contains("alpha"),
        "the top match is selected instead of an arbitrary leftover row:\n{line}"
    );
}

#[test]
fn esc_clears_a_submitted_filter_and_restores_the_full_list() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('/'));
    for ch in "feat".chars() {
        press(&mut app, &mut input, KeyCode::Char(ch));
    }
    press(&mut app, &mut input, KeyCode::Enter);
    let filtered = panel_region(&draw(&mut app, 80, 20));
    assert!(
        !filtered.contains("main"),
        "the submitted filter still hides non-matches:\n{filtered}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    let restored = panel_region(&draw(&mut app, 80, 20));
    assert!(
        restored.contains("main"),
        "Esc clears the submitted filter and restores the full list:\n{restored}"
    );
    assert!(
        restored.contains("feature"),
        "the full list is back:\n{restored}"
    );
}

#[test]
fn the_branch_panel_marks_hidden_and_pinned_branches() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    press(&mut app, &mut input, KeyCode::Char('k'));
    press(&mut app, &mut input, KeyCode::Char('p'));
    settle(&mut app);

    let screen = dump(&draw(&mut app, 80, 20));
    assert!(
        screen.contains("feature"),
        "a hidden branch stays listed in the panel:\n{screen}"
    );
    assert!(
        screen.contains('◌'),
        "a hidden branch carries a hidden marker:\n{screen}"
    );
    assert!(
        screen.contains('★'),
        "a pinned branch carries a pin marker:\n{screen}"
    );
}

#[test]
fn a_pinned_branch_survives_soloing_another() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('p'));
    press(&mut app, &mut input, KeyCode::Char('k'));
    press(&mut app, &mut input, KeyCode::Char('o'));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);

    let after = dump(&draw(&mut app, 80, 20));
    assert!(
        after.contains("Work on feature"),
        "the pinned feature branch survives soloing main:\n{after}"
    );
    assert!(
        after.contains("Add feature base"),
        "the soloed main keeps its commit:\n{after}"
    );
}

#[test]
fn soloing_a_branch_hides_every_other_branch_from_the_graph() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('o'));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);

    let after = dump(&draw(&mut app, 80, 20));
    assert!(
        after.contains("Add feature base"),
        "the soloed HEAD branch keeps its commit:\n{after}"
    );
    assert!(
        !after.contains("Work on feature"),
        "soloing main hides the feature branch's exclusive commit:\n{after}"
    );
}

#[test]
fn j_moves_the_branch_panel_marker_down_one_row() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    let before = panel_marker_row(&draw(&mut app, 80, 20)).expect("marker visible on open");

    press(&mut app, &mut input, KeyCode::Char('j'));
    let after = panel_marker_row(&draw(&mut app, 80, 20)).expect("marker still visible");

    assert_eq!(
        after,
        before + 1,
        "the focus marker should slide down exactly one branch row"
    );
}

#[test]
fn enter_checks_out_the_focused_branch() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);

    assert_eq!(
        app.meta().head_branch.as_deref(),
        Some("feature"),
        "HEAD should have moved to the checked-out branch"
    );
    let screen = dump(&draw(&mut app, 80, 20));
    assert!(
        screen.contains("⎇ feature"),
        "the status bar should show the new branch:\n{screen}"
    );
}

#[test]
fn undo_returns_to_the_previous_branch_after_checkout() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    assert_eq!(app.meta().head_branch.as_deref(), Some("feature"));

    press(&mut app, &mut input, KeyCode::Char('u'));
    settle(&mut app);

    assert_eq!(
        app.meta().head_branch.as_deref(),
        Some("main"),
        "undo should return to the branch we came from"
    );
}

#[test]
fn n_creates_a_branch_and_switches_to_it() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('n'));
    for character in "spike".chars() {
        press(&mut app, &mut input, KeyCode::Char(character));
    }
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);

    assert_eq!(
        app.meta().head_branch.as_deref(),
        Some("spike"),
        "a created branch should become the current one"
    );
    let screen = dump(&draw(&mut app, 80, 20));
    assert!(
        screen.contains("spike"),
        "the new branch should show as a badge:\n{screen}"
    );
}

#[test]
fn undo_drops_a_created_branch_and_returns_to_the_previous_one() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('n'));
    for character in "spike".chars() {
        press(&mut app, &mut input, KeyCode::Char(character));
    }
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    assert_eq!(app.meta().head_branch.as_deref(), Some("spike"));

    press(&mut app, &mut input, KeyCode::Char('u'));
    settle(&mut app);

    assert_eq!(
        app.meta().head_branch.as_deref(),
        Some("main"),
        "undo should switch back to the previous branch"
    );
    let has_spike = app
        .meta()
        .badges
        .values()
        .flatten()
        .any(|badge| badge.label == "spike");
    assert!(!has_spike, "the created branch should be gone");
}

fn delete_repo(dir: &Path) {
    let run = |args: &[&str]| git_run(dir, args);

    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "demo@ravix.dev"]);
    run(&["config", "user.name", "Demo"]);

    std::fs::write(dir.join("app.txt"), "one\n").unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "one"]);
    run(&["branch", "stale"]);

    run(&["switch", "-c", "feature"]);
    std::fs::write(dir.join("feature.txt"), "wip\n").unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "feature work"]);

    run(&["switch", "main"]);
    std::fs::write(dir.join("app.txt"), "one\ntwo\n").unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "two"]);
}

fn branch_labels(app: &App) -> Vec<String> {
    app.meta()
        .badges
        .values()
        .flatten()
        .map(|badge| badge.label.clone())
        .collect()
}

#[test]
fn d_deletes_a_merged_branch_after_confirmation() {
    let dir = TempDir::new().unwrap();
    delete_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('d'));
    assert!(
        app.confirm().is_some(),
        "delete should ask for confirmation"
    );
    press(&mut app, &mut input, KeyCode::Char('y'));
    settle(&mut app);

    assert!(
        !branch_labels(&app).contains(&"stale".to_string()),
        "the merged branch should be gone: {:?}",
        branch_labels(&app)
    );
}

#[test]
fn deleting_an_unmerged_branch_escalates_to_a_force_confirmation() {
    let dir = TempDir::new().unwrap();
    delete_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('d'));
    press(&mut app, &mut input, KeyCode::Char('y'));

    let confirm = app.confirm().expect("an unmerged delete should escalate");
    assert!(
        confirm.message.contains("Force"),
        "the escalation should ask to force: {}",
        confirm.message
    );

    press(&mut app, &mut input, KeyCode::Char('y'));
    settle(&mut app);
    assert!(
        !branch_labels(&app).contains(&"feature".to_string()),
        "forcing should delete the unmerged branch: {:?}",
        branch_labels(&app)
    );
}

#[test]
fn deleting_the_current_branch_is_blocked() {
    let dir = TempDir::new().unwrap();
    delete_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('d'));

    assert!(
        app.confirm().is_none(),
        "no confirmation for the current branch"
    );
    assert!(
        app.alert().is_some(),
        "a blocking alert modal should appear"
    );
    let screen = dump(&draw(&mut app, 80, 20));
    assert!(
        screen.contains("Blocked") && screen.contains("current branch"),
        "the alert modal should explain the block:\n{screen}"
    );
    assert!(
        branch_labels(&app).contains(&"main".to_string()),
        "main should still exist"
    );

    press(&mut app, &mut input, KeyCode::Char('x'));
    assert!(app.alert().is_none(), "any key should dismiss the alert");
}

#[test]
fn hiding_a_merged_branch_removes_its_pill_but_keeps_the_commit() {
    let dir = TempDir::new().unwrap();
    delete_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    let before = dump(&draw(&mut app, 100, 24));
    assert!(
        before.contains("stale"),
        "the merged branch pill shows while visible:\n{before}"
    );

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);

    let after = dump(&draw(&mut app, 100, 24));
    assert!(
        after.contains("one"),
        "the merged commit stays — it belongs to main's history:\n{after}"
    );
    assert!(
        !after.contains("stale"),
        "the hidden branch's pill is gone from the rail:\n{after}"
    );
}

#[test]
fn undo_recreates_a_deleted_branch() {
    let dir = TempDir::new().unwrap();
    delete_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('d'));
    press(&mut app, &mut input, KeyCode::Char('y'));
    settle(&mut app);
    assert!(!branch_labels(&app).contains(&"stale".to_string()));

    press(&mut app, &mut input, KeyCode::Char('u'));
    settle(&mut app);

    assert!(
        branch_labels(&app).contains(&"stale".to_string()),
        "undo should recreate the deleted branch: {:?}",
        branch_labels(&app)
    );
}

#[test]
fn space_on_the_graph_checks_out_a_branch_at_the_selected_commit() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    settle(&mut app);

    assert_eq!(
        app.meta().head_branch.as_deref(),
        Some("feature"),
        "selecting the feature commit and checking out should attach to its branch"
    );
}

#[test]
fn space_on_a_branchless_commit_checks_out_detached() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('G'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    settle(&mut app);

    assert_eq!(
        app.meta().head_branch,
        None,
        "checking out a branchless commit should detach HEAD"
    );
}

#[test]
fn checking_out_reports_the_branch_you_landed_on() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);

    assert_eq!(app.notice(), Some("On feature"));
}

#[test]
fn a_detached_checkout_reports_it() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('G'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    settle(&mut app);

    let notice = app.notice().expect("a detached checkout should report");
    assert!(
        notice.starts_with("Detached HEAD at"),
        "notice should announce the detached HEAD: {notice}"
    );
}

#[test]
fn selecting_a_commit_with_extra_refs_expands_them_below_the_rail() {
    let dir = TempDir::new().unwrap();
    two_commit_repo(dir.path());
    git_run(dir.path(), &["branch", "twin"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    let selected = dump(&draw(&mut app, 80, 20));
    assert!(
        selected.contains("+1"),
        "the extra ref shows as a +1 counter:\n{selected}"
    );
    assert!(
        selected.contains("twin"),
        "the extra ref expands below the rail while its commit is selected:\n{selected}"
    );

    press(&mut app, &mut input, KeyCode::Char('j'));
    let moved = dump(&draw(&mut app, 80, 20));
    assert!(
        moved.contains("+1"),
        "the +1 counter stays when the selection moves away:\n{moved}"
    );
    assert!(
        !moved.contains("twin"),
        "the expansion collapses when the selection moves away:\n{moved}"
    );
}

#[test]
fn the_current_branch_shows_checked_in_the_ref_rail() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();

    let screen = dump(&draw(&mut app, 80, 20));

    assert!(
        screen.contains("✓ main"),
        "the checked-out branch pill should read as ✓ main:\n{screen}"
    );
}

fn checkout_feature(app: &mut App, input: &mut InputMap) {
    press(app, input, KeyCode::Char('b'));
    settle(app);
    press(app, input, KeyCode::Char('j'));
    press(app, input, KeyCode::Enter);
}

fn cycle_celebrations(app: &mut App, input: &mut InputMap) {
    press_ctrl(app, input, KeyCode::Char('p'));
    type_text(app, input, "cycle");
    press(app, input, KeyCode::Enter);
}

#[test]
fn a_celebrated_action_sparkles_then_settles() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    checkout_feature(&mut app, &mut input);

    assert!(
        app.celebration().is_some(),
        "a fresh checkout should trigger a celebration"
    );
    let screen = dump(&draw(&mut app, 100, 24));
    assert!(
        screen.contains('✦'),
        "Full intensity should render the sparkle burst:\n{screen}"
    );

    settle(&mut app);
    assert!(
        app.celebration().is_none(),
        "the celebration should decay back to rest"
    );
    let after = dump(&draw(&mut app, 100, 24));
    assert!(
        !after.contains('✦'),
        "the sparkle should be gone once it settles:\n{after}"
    );
}

#[test]
fn committing_also_celebrates() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('c'));
    type_text(&mut app, &mut input, "land it");
    press(&mut app, &mut input, KeyCode::Enter);

    assert!(
        app.celebration().is_some(),
        "landing a commit should trigger a celebration"
    );
}

#[test]
fn cycling_the_dial_from_the_palette_reports_the_new_level() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    cycle_celebrations(&mut app, &mut input);

    assert_eq!(app.notice(), Some("Celebrations: subtle"));
    assert!(
        app.palette().is_none(),
        "running the command should close the palette"
    );
}

#[test]
fn at_subtle_intensity_a_celebrated_action_pulses_without_sparkles() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    cycle_celebrations(&mut app, &mut input);
    checkout_feature(&mut app, &mut input);

    assert!(app.celebration().is_some(), "Subtle should still celebrate");
    let screen = dump(&draw(&mut app, 100, 24));
    assert!(
        !screen.contains('✦'),
        "Subtle intensity should not render sparkles:\n{screen}"
    );
}

#[test]
fn at_off_intensity_a_celebrated_action_does_not_celebrate() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    cycle_celebrations(&mut app, &mut input);
    cycle_celebrations(&mut app, &mut input);
    checkout_feature(&mut app, &mut input);

    assert!(
        app.celebration().is_none(),
        "Off should suppress the celebration entirely"
    );
    let screen = dump(&draw(&mut app, 100, 24));
    assert!(
        !screen.contains('✦'),
        "Off intensity should render no sparkles:\n{screen}"
    );
}

#[test]
fn a_celebration_opens_no_input_context() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('G'));
    press(&mut app, &mut input, KeyCode::Char(' '));

    assert!(app.celebration().is_some(), "a checkout should celebrate");
    assert_eq!(
        app.input_context(),
        InputContext::Graph,
        "a celebration is purely visual and must not capture input"
    );
}

fn upstream_repo(work: &Path, remote: &Path) {
    let run_in = |dir: &Path, args: &[&str]| {
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

    run_in(remote, &["init", "-q", "--bare", "-b", "main"]);
    run_in(work, &["init", "-q", "-b", "main"]);
    run_in(work, &["config", "user.email", "demo@ravix.dev"]);
    run_in(work, &["config", "user.name", "Demo"]);

    std::fs::write(work.join("app.txt"), "one\n").unwrap();
    run_in(work, &["add", "-A"]);
    run_in(work, &["commit", "-q", "-m", "one"]);
    run_in(work, &["remote", "add", "origin", remote.to_str().unwrap()]);
    run_in(work, &["push", "-q", "-u", "origin", "main"]);

    std::fs::write(work.join("app.txt"), "one\ntwo\n").unwrap();
    run_in(work, &["add", "-A"]);
    run_in(work, &["commit", "-q", "-m", "two"]);
}

#[test]
fn the_branch_list_shows_ahead_behind_against_upstream() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    upstream_repo(work.path(), remote.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    let screen = dump(&draw(&mut app, 100, 24));

    assert!(
        screen.contains("↑1"),
        "the branch list should show the branch is one commit ahead of its upstream:\n{screen}"
    );
}

fn conflicting_checkout_repo(dir: &Path) {
    let run = |args: &[&str]| git_run(dir, args);

    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "demo@ravix.dev"]);
    run(&["config", "user.name", "Demo"]);

    std::fs::write(dir.join("app.txt"), "base\n").unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "base"]);

    run(&["switch", "-c", "feature"]);
    std::fs::write(dir.join("app.txt"), "feature version\n").unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "feature"]);

    run(&["switch", "main"]);
    std::fs::write(dir.join("app.txt"), "dirty edit\n").unwrap();
}

#[test]
fn checkout_that_git_refuses_shows_the_error_and_stays_put() {
    let dir = TempDir::new().unwrap();
    conflicting_checkout_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);

    assert_eq!(
        app.meta().head_branch.as_deref(),
        Some("main"),
        "a refused checkout must not move HEAD"
    );
    assert!(
        app.branch_panel().is_some(),
        "the branch list should stay open after a refused checkout"
    );
    assert!(
        app.notice().is_some(),
        "git's refusal should surface as a notice"
    );
}

#[test]
fn m_opens_the_join_menu_with_a_merge_prediction() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('M'));
    settle(&mut app);
    let screen = dump(&draw(&mut app, 100, 28));

    assert!(
        screen.contains("Join feature into HEAD"),
        "join menu title missing:\n{screen}"
    );
    assert!(
        screen.contains("Merge commit"),
        "merge strategy missing:\n{screen}"
    );
    assert!(
        screen.contains("clean"),
        "the merge-tree prediction should annotate the strategy:\n{screen}"
    );
}

fn git_run(dir: &Path, args: &[&str]) {
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
}

fn init_with_base(dir: &Path, file: &str, content: &str) {
    git_run(dir, &["init", "-q", "-b", "main"]);
    git_run(dir, &["config", "user.email", "demo@ravix.dev"]);
    git_run(dir, &["config", "user.name", "Demo"]);
    std::fs::write(dir.join(file), content).unwrap();
    git_run(dir, &["add", "-A"]);
    git_run(dir, &["commit", "-q", "-m", "base"]);
}

fn commit_file(dir: &Path, file: &str, content: &str, message: &str) {
    std::fs::write(dir.join(file), content).unwrap();
    git_run(dir, &["add", "-A"]);
    git_run(dir, &["commit", "-q", "-m", message]);
}

fn merge_repo(dir: &Path) {
    init_with_base(dir, "a.txt", "a\n");
    git_run(dir, &["switch", "-qc", "feature"]);
    commit_file(dir, "feature.txt", "f\n", "feature work");
    git_run(dir, &["switch", "-q", "main"]);
    commit_file(dir, "main.txt", "m\n", "main work");
}

fn ff_repo(dir: &Path) {
    init_with_base(dir, "a.txt", "a\n");
    git_run(dir, &["switch", "-qc", "feature"]);
    commit_file(dir, "a.txt", "a\nmore\n", "ahead");
    git_run(dir, &["switch", "-q", "main"]);
}

fn conflict_repo(dir: &Path) {
    init_with_base(dir, "app.txt", "base\n");
    git_run(dir, &["switch", "-qc", "feature"]);
    commit_file(dir, "app.txt", "feature\n", "feature edit");
    git_run(dir, &["switch", "-q", "main"]);
    commit_file(dir, "app.txt", "main\n", "main edit");
}

fn open_join_on_feature(app: &mut App, input: &mut InputMap) {
    press(app, input, KeyCode::Char('b'));
    settle(app);
    press(app, input, KeyCode::Char('j'));
    press(app, input, KeyCode::Char('M'));
    settle(app);
}

#[test]
fn fast_forward_advances_the_current_branch_linearly() {
    let dir = TempDir::new().unwrap();
    ff_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_join_on_feature(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);

    let head = app.head_commit().expect("HEAD commit");
    assert_eq!(app.meta().head_branch.as_deref(), Some("main"));
    assert_eq!(
        head.summary, "ahead",
        "the current branch should have fast-forwarded to the source tip"
    );
    assert_eq!(head.parents.len(), 1, "a fast-forward stays linear");
}

#[test]
fn a_clean_merge_creates_a_merge_commit() {
    let dir = TempDir::new().unwrap();
    merge_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_join_on_feature(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);

    let head = app.head_commit().expect("HEAD commit");
    assert_eq!(
        head.parents.len(),
        2,
        "a --no-ff merge should produce a two-parent commit:\n{head:?}"
    );
}

#[test]
fn cherry_pick_lands_the_source_change_on_head() {
    let dir = TempDir::new().unwrap();
    merge_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_join_on_feature(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);

    let head = app.head_commit().expect("HEAD commit");
    assert_eq!(
        head.summary, "feature work",
        "cherry-pick should replay the source commit onto HEAD"
    );
    assert_eq!(
        head.parents.len(),
        1,
        "a cherry-pick is a single-parent commit"
    );
}

#[test]
fn a_conflict_prediction_still_lists_the_files_in_the_menu() {
    let dir = TempDir::new().unwrap();
    conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_join_on_feature(&mut app, &mut input);
    let screen = dump(&draw(&mut app, 100, 28));
    assert!(
        screen.contains("Conflicts") && screen.contains("app.txt"),
        "the conflict prediction should list the files:\n{screen}"
    );
}

#[test]
fn a_conflict_merge_proceeds_into_the_conflict_browser() {
    let dir = TempDir::new().unwrap();
    conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_join_on_feature(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);

    assert!(
        app.conflict_browser().is_some(),
        "a conflict merge should open the conflict browser instead of blocking"
    );
    let screen = dump(&draw(&mut app, 100, 28));
    assert!(
        screen.contains("merge in progress"),
        "the browser should show the in-progress operation:\n{screen}"
    );
    assert!(
        screen.contains("app.txt") && screen.contains("OURS") && screen.contains("THEIRS"),
        "the browser should render the conflicted file 3-way:\n{screen}"
    );
}

#[test]
fn resolving_every_block_and_continuing_completes_the_merge() {
    let dir = TempDir::new().unwrap();
    conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_join_on_feature(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    assert!(app.conflict_browser().is_some());

    press(&mut app, &mut input, KeyCode::Char('o'));
    press(&mut app, &mut input, KeyCode::Char('c'));
    press(&mut app, &mut input, KeyCode::Enter);

    assert!(
        app.conflict_browser().is_none(),
        "continuing should leave the browser"
    );
    assert_eq!(
        app.head_commit().expect("HEAD commit").parents.len(),
        2,
        "the resolved merge should produce a two-parent commit"
    );
    let content = std::fs::read_to_string(dir.path().join("app.txt")).unwrap();
    assert!(
        content.contains("main"),
        "taking ours should have kept HEAD's side:\n{content}"
    );
}

#[test]
fn continue_opens_an_editable_commit_message() {
    let dir = TempDir::new().unwrap();
    conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_join_on_feature(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    press(&mut app, &mut input, KeyCode::Char('o'));
    press(&mut app, &mut input, KeyCode::Char('c'));

    let editor = app
        .commit_editor()
        .expect("continue should open a commit editor");
    assert!(
        !editor.message.trim().is_empty(),
        "the editor should be prefilled with the default message"
    );

    for character in " EDITED".chars() {
        press(&mut app, &mut input, KeyCode::Char(character));
    }
    press(&mut app, &mut input, KeyCode::Enter);

    let head = app.head_commit().expect("HEAD commit");
    assert_eq!(head.parents.len(), 2, "the merge should have completed");
    assert!(
        head.summary.contains("EDITED"),
        "the custom commit message should be used: {}",
        head.summary
    );
}

#[test]
fn editing_externally_resolves_a_conflict_when_markers_are_removed() {
    let dir = TempDir::new().unwrap();
    conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_join_on_feature(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    assert!(app.conflict_browser().is_some());

    press(&mut app, &mut input, KeyCode::Char('e'));
    let path = app
        .pending_edit_path()
        .expect("[e] should request an external edit");
    assert!(path.ends_with("app.txt"));

    std::fs::write(&path, "combined by hand\n").unwrap();
    app.finish_edit();

    assert!(
        app.conflict_browser().is_some_and(|b| b.files.is_empty()),
        "a manually resolved file should drop out of the browser"
    );

    press(&mut app, &mut input, KeyCode::Char('c'));
    press(&mut app, &mut input, KeyCode::Enter);

    assert_eq!(app.head_commit().expect("HEAD commit").parents.len(), 2);
    let content = std::fs::read_to_string(dir.path().join("app.txt")).unwrap();
    assert!(
        content.contains("combined by hand"),
        "the manual resolution should be committed:\n{content}"
    );
}

#[test]
fn aborting_a_conflict_returns_to_the_pre_op_commit() {
    let dir = TempDir::new().unwrap();
    conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_join_on_feature(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    assert!(app.conflict_browser().is_some());

    press(&mut app, &mut input, KeyCode::Esc);

    assert!(
        app.conflict_browser().is_none(),
        "Esc should abort and leave the browser"
    );
    assert_eq!(
        app.head_commit().expect("HEAD commit").summary,
        "main edit",
        "abort should return to the pre-merge commit"
    );
    assert!(
        !app.has_wip(),
        "the working tree should be clean after an abort"
    );
}

#[test]
fn a_conflict_cherry_pick_resolves_through_the_browser() {
    let dir = TempDir::new().unwrap();
    conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_join_on_feature(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);

    assert!(
        app.conflict_browser().is_some(),
        "a conflicting cherry-pick should open the browser"
    );
    let screen = dump(&draw(&mut app, 100, 28));
    assert!(
        screen.contains("cherry-pick in progress"),
        "the browser should show a cherry-pick in progress:\n{screen}"
    );

    press(&mut app, &mut input, KeyCode::Char('t'));
    press(&mut app, &mut input, KeyCode::Char('c'));
    press(&mut app, &mut input, KeyCode::Enter);

    assert!(app.conflict_browser().is_none());
    let head = app.head_commit().expect("HEAD commit");
    assert_eq!(
        head.summary, "feature edit",
        "the cherry-picked commit should land on HEAD with its default message"
    );
    assert_eq!(head.parents.len(), 1, "a cherry-pick is single-parent");
}

#[test]
fn an_in_progress_merge_is_detected_when_ravix_opens() {
    let dir = TempDir::new().unwrap();
    conflict_repo(dir.path());
    let out = Command::new("git")
        .current_dir(dir.path())
        .args(["merge", "--no-ff", "feature"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "the merge should have conflicted");

    let mut app = App::open(dir.path()).unwrap();

    assert!(
        app.conflict_browser().is_some(),
        "an already-conflicted merge should open the browser on startup"
    );
    let screen = dump(&draw(&mut app, 100, 28));
    assert!(
        screen.contains("app.txt"),
        "the conflicted file should be shown:\n{screen}"
    );
}

#[test]
fn a_clean_strategy_shows_a_ghost_topology_preview() {
    let dir = TempDir::new().unwrap();
    merge_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_join_on_feature(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('j'));
    let screen = dump(&draw(&mut app, 140, 28));

    assert!(
        screen.contains("preview"),
        "a ghost preview should appear for a clean strategy:\n{screen}"
    );
    assert!(
        screen.contains("◈") && screen.contains("╱ ╲"),
        "the ghost should draw a phantom merge node and its edges:\n{screen}"
    );
    assert!(
        screen.contains("+1 commits · merge commit · 2 parents · clean"),
        "the summary should describe the resulting topology with the incoming count:\n{screen}"
    );
}

#[test]
fn undo_reverts_a_merge() {
    let dir = TempDir::new().unwrap();
    merge_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_join_on_feature(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    assert_eq!(
        app.head_commit().expect("HEAD commit").parents.len(),
        2,
        "merge should have landed"
    );

    press(&mut app, &mut input, KeyCode::Char('u'));
    settle(&mut app);

    let head = app.head_commit().expect("HEAD commit");
    assert_eq!(
        head.summary, "main work",
        "undo should reset HEAD back to the pre-merge tip"
    );
    assert_eq!(head.parents.len(), 1, "the merge commit should be gone");
}

fn mouse(app: &mut App, input: &mut InputMap, kind: MouseEventKind, row: u16) {
    let event = MouseEvent {
        kind,
        column: 3,
        row,
        modifiers: KeyModifiers::NONE,
    };
    let graph_area = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 27,
    };
    if let Some(action) = input.on_mouse(event, graph_area, app.input_context()) {
        app.update(action);
    }
}

fn drag_row_onto(app: &mut App, input: &mut InputMap, from: u16, to: u16) {
    mouse(app, input, MouseEventKind::Down(MouseButton::Left), from);
    mouse(app, input, MouseEventKind::Drag(MouseButton::Left), to);
    mouse(app, input, MouseEventKind::Up(MouseButton::Left), to);
}

#[test]
fn dropping_a_branch_onto_head_opens_the_join_menu() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    drag_row_onto(&mut app, &mut input, 2, 1);

    assert_eq!(
        app.meta().head_branch.as_deref(),
        Some("main"),
        "dropping onto the current branch needs no checkout"
    );
    assert_eq!(
        app.join_menu().map(|menu| menu.title.as_str()),
        Some("Join feature into HEAD"),
        "the drop should open the join menu with the dragged branch as source"
    );
}

#[test]
fn dropping_onto_another_branch_checks_it_out_first() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    drag_row_onto(&mut app, &mut input, 1, 2);

    assert_eq!(
        app.meta().head_branch.as_deref(),
        Some("feature"),
        "dropping main onto feature should check feature out"
    );
    assert_eq!(
        app.join_menu().map(|menu| menu.title.as_str()),
        Some("Join main into HEAD"),
        "the join menu should integrate the dragged branch into the new HEAD"
    );
}

#[test]
fn undo_reverts_the_integration_a_drop_produced() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    drag_row_onto(&mut app, &mut input, 2, 1);
    assert!(
        app.join_menu().is_some(),
        "the drop should have opened the join menu"
    );

    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    assert_eq!(
        app.head_commit().expect("HEAD commit").parents.len(),
        2,
        "the dropped merge should land"
    );

    press(&mut app, &mut input, KeyCode::Char('u'));

    assert_eq!(
        app.head_commit().expect("HEAD commit").parents.len(),
        1,
        "u should undo the drop's integration"
    );
}

#[test]
fn releasing_on_the_same_row_selects_instead_of_dropping() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Down(MouseButton::Left),
        2,
    );
    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Up(MouseButton::Left),
        2,
    );

    assert!(
        app.join_menu().is_none(),
        "a click should not open the join menu"
    );
    assert_eq!(app.selected(), 1, "a click should select the row");
}

fn two_commit_dirty_repo(dir: &Path) {
    init_with_base(dir, "a.txt", "1\n");
    commit_file(dir, "b.txt", "2\n", "second");
    std::fs::write(dir.join("a.txt"), "dirty\n").unwrap();
}

#[test]
fn pointer_targeting_accounts_for_the_wip_row() {
    let dir = TempDir::new().unwrap();
    two_commit_dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    assert!(
        app.has_wip(),
        "the tree should be dirty so a WIP row is present"
    );

    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Down(MouseButton::Left),
        2,
    );
    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Up(MouseButton::Left),
        2,
    );
    assert_eq!(
        app.selected(),
        0,
        "visible row 2 (below the WIP row and the header) is the first commit"
    );

    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Down(MouseButton::Left),
        3,
    );
    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Up(MouseButton::Left),
        3,
    );
    assert_eq!(app.selected(), 1, "visible row 3 is the second commit");
}

#[test]
fn dragging_from_a_branchless_commit_does_nothing() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    drag_row_onto(&mut app, &mut input, 3, 1);

    assert!(
        app.join_menu().is_none(),
        "dragging a non-branch commit should not open the join menu"
    );
}

#[test]
fn the_drag_shows_a_ghost_over_the_target_row() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Down(MouseButton::Left),
        2,
    );
    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Drag(MouseButton::Left),
        1,
    );
    let screen = dump(&draw(&mut app, 100, 28));

    assert!(
        screen.contains("drop feature"),
        "a drag should show the grabbed branch over the target row:\n{screen}"
    );
}

#[test]
fn a_drop_whose_checkout_git_refuses_cancels_the_drop() {
    let dir = TempDir::new().unwrap();
    conflicting_checkout_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    // A WIP row sits at visible row 0 and the header at row 1; feature is row 2, main (HEAD) row 3.
    assert!(app.has_wip());
    drag_row_onto(&mut app, &mut input, 3, 2);

    assert!(
        app.join_menu().is_none(),
        "a refused checkout should cancel the drop, not open the menu"
    );
    assert_eq!(
        app.meta().head_branch.as_deref(),
        Some("main"),
        "a refused drop must not move HEAD"
    );
    assert!(
        app.notice().is_some(),
        "the refusal should surface as a notice"
    );
}

fn git_clone(remote: &Path, dest: &Path) {
    let out = Command::new("git")
        .args([
            "clone",
            "-q",
            remote.to_str().unwrap(),
            dest.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git clone: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn unpushed_repo(work: &Path, remote: &Path) {
    git_run(remote, &["init", "-q", "--bare", "-b", "main"]);
    init_with_base(work, "a.txt", "a\n");
    git_run(work, &["remote", "add", "origin", remote.to_str().unwrap()]);
}

fn diverged_repo(work: &Path, remote: &Path, other: &Path) {
    unpushed_repo(work, remote);
    git_run(work, &["push", "-q", "-u", "origin", "main"]);

    git_clone(remote, other);
    git_run(other, &["config", "user.email", "demo@ravix.dev"]);
    git_run(other, &["config", "user.name", "Demo"]);
    commit_file(other, "b.txt", "b\n", "remote work");
    git_run(other, &["push", "-q"]);

    commit_file(work, "c.txt", "c\n", "local work");
    git_run(work, &["fetch", "-q"]);
}

fn drive_remote(app: &mut App) {
    let mut guard = 0;
    while app.remote_pending() {
        app.poll_remote();
        std::thread::yield_now();
        guard += 1;
        assert!(guard < 1_000_000, "the remote op never completed");
    }
}

#[test]
fn fetch_runs_in_the_background_with_a_spinner() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    upstream_repo(work.path(), remote.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('f'));
    assert!(app.remote_pending(), "fetch should start a background job");
    let screen = dump(&draw(&mut app, 100, 24));
    assert!(
        screen.contains("fetching"),
        "the spinner should show the running op:\n{screen}"
    );

    drive_remote(&mut app);
    assert!(!app.remote_pending(), "the job should complete");
    assert_eq!(app.notice(), Some("Fetched"));
}

#[test]
fn the_status_bar_reports_ahead_behind() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    upstream_repo(work.path(), remote.path());
    let mut app = App::open(work.path()).unwrap();

    let screen = dump(&draw(&mut app, 100, 24));
    assert!(
        screen.contains("↑1 ↓0"),
        "the status bar should report ahead/behind vs upstream:\n{screen}"
    );
}

#[test]
fn push_publishes_the_branch_and_syncs_the_upstream() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    upstream_repo(work.path(), remote.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();
    assert_eq!(
        app.head_tracking(),
        Some((1, 0)),
        "the fixture is one ahead"
    );

    press(&mut app, &mut input, KeyCode::Char('P'));
    assert!(app.remote_pending(), "push should start a background job");
    drive_remote(&mut app);

    assert_eq!(app.notice(), Some("Pushed"));
    assert_eq!(
        app.head_tracking(),
        Some((0, 0)),
        "after pushing, the branch is in sync with its upstream"
    );
}

#[test]
fn a_first_push_sets_the_upstream() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    unpushed_repo(work.path(), remote.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();
    assert_eq!(app.head_tracking(), None, "no upstream yet");

    press(&mut app, &mut input, KeyCode::Char('P'));
    drive_remote(&mut app);

    assert_eq!(app.notice(), Some("Pushed"));
    assert!(
        app.head_tracking().is_some(),
        "the first push should set the upstream"
    );
}

#[test]
fn a_diverged_push_asks_to_force_first() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let other = TempDir::new().unwrap();
    diverged_repo(work.path(), remote.path(), other.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('P'));

    assert!(
        !app.remote_pending(),
        "a diverged push must not push blindly"
    );
    let confirm = app.confirm().expect("a diverged push should ask to force");
    assert!(
        confirm.message.contains("force"),
        "the confirmation should offer force-with-lease: {}",
        confirm.message
    );

    press(&mut app, &mut input, KeyCode::Char('y'));
    drive_remote(&mut app);
    assert_eq!(app.notice(), Some("Pushed"));
}

fn behind_repo(work: &Path, remote: &Path, other: &Path) {
    unpushed_repo(work, remote);
    git_run(work, &["push", "-q", "-u", "origin", "main"]);
    git_clone(remote, other);
    git_run(other, &["config", "user.email", "demo@ravix.dev"]);
    git_run(other, &["config", "user.name", "Demo"]);
    commit_file(other, "b.txt", "b\n", "remote work");
    git_run(other, &["push", "-q"]);
}

fn diverged_conflict_repo(work: &Path, remote: &Path, other: &Path) {
    git_run(remote, &["init", "-q", "--bare", "-b", "main"]);
    init_with_base(work, "shared.txt", "base\n");
    git_run(work, &["remote", "add", "origin", remote.to_str().unwrap()]);
    git_run(work, &["push", "-q", "-u", "origin", "main"]);
    git_clone(remote, other);
    git_run(other, &["config", "user.email", "demo@ravix.dev"]);
    git_run(other, &["config", "user.name", "Demo"]);
    commit_file(other, "shared.txt", "remote\n", "remote edit");
    git_run(other, &["push", "-q"]);
    commit_file(work, "shared.txt", "local\n", "local edit");
    git_run(work, &["fetch", "-q"]);
}

#[test]
fn pull_fast_forwards_when_the_upstream_advanced() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let other = TempDir::new().unwrap();
    behind_repo(work.path(), remote.path(), other.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('p'));
    assert!(app.remote_pending(), "pull should start a background fetch");
    drive_remote(&mut app);

    assert_eq!(
        app.head_commit().expect("HEAD commit").summary,
        "remote work",
        "a fast-forward pull should advance onto the upstream"
    );
    assert!(
        app.notice().is_some_and(|n| n.contains("fast-forward")),
        "the notice should report the fast-forward: {:?}",
        app.notice()
    );
}

#[test]
fn pull_reports_up_to_date_when_nothing_is_upstream() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    upstream_repo(work.path(), remote.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('p'));
    drive_remote(&mut app);

    assert_eq!(app.notice(), Some("Already up to date"));
    assert!(app.join_menu().is_none());
}

#[test]
fn a_diverged_pull_opens_the_join_menu_with_the_upstream() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let other = TempDir::new().unwrap();
    diverged_repo(work.path(), remote.path(), other.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('p'));
    drive_remote(&mut app);

    assert_eq!(
        app.join_menu().map(|menu| menu.title.as_str()),
        Some("Join origin/main into HEAD"),
        "a diverged pull should open the join menu with the upstream as source"
    );
}

#[test]
fn a_conflicting_pull_lands_in_the_conflict_browser() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let other = TempDir::new().unwrap();
    diverged_conflict_repo(work.path(), remote.path(), other.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('p'));
    drive_remote(&mut app);
    assert!(
        app.join_menu().is_some(),
        "a diverged pull opens the join menu"
    );

    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);

    assert!(
        app.conflict_browser().is_some(),
        "a conflicting merge pull should drop into the conflict browser"
    );
}

#[test]
fn pull_without_an_upstream_is_refused() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    unpushed_repo(work.path(), remote.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('p'));

    assert!(!app.remote_pending(), "no upstream — nothing to fetch");
    assert!(app.notice().is_some_and(|n| n.contains("upstream")));
    assert!(app.join_menu().is_none());
}

#[test]
fn undo_reverts_a_fast_forward_pull() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let other = TempDir::new().unwrap();
    behind_repo(work.path(), remote.path(), other.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('p'));
    drive_remote(&mut app);
    assert_eq!(
        app.head_commit().expect("HEAD commit").summary,
        "remote work"
    );

    press(&mut app, &mut input, KeyCode::Char('u'));

    assert_eq!(
        app.head_commit().expect("HEAD commit").summary,
        "base",
        "undo should take the pull back off the upstream"
    );
}

#[test]
fn s_stashes_the_working_changes() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    assert!(app.has_wip(), "the fixture starts dirty");

    press(&mut app, &mut input, KeyCode::Char('s'));

    assert!(!app.has_wip(), "stashing should clear the working tree");
    assert_eq!(app.notice(), Some("Stashed working changes"));
}

#[test]
fn opening_the_stash_list_shows_the_saved_stash() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('s'));
    press(&mut app, &mut input, KeyCode::Char('S'));
    settle(&mut app);
    let screen = dump(&draw(&mut app, 100, 24));

    assert!(
        screen.contains("Stashes"),
        "stash panel title missing:\n{screen}"
    );
    assert!(
        screen.contains("no message"),
        "an unnamed stash should say so:\n{screen}"
    );
}

fn stash_repo(dir: &Path, message: &str) {
    dirty_repo(dir);
    git_run(dir, &["stash", "push", "-u", "-m", message]);
}

#[test]
fn a_stash_card_shows_its_message_branch_and_size() {
    let dir = TempDir::new().unwrap();
    stash_repo(dir.path(), "fix status refresh race");
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('S'));
    settle(&mut app);
    let screen = dump(&draw(&mut app, 120, 24));

    assert!(
        screen.contains("fix status refresh race"),
        "the card should show the message it was saved with:\n{screen}"
    );
    assert!(
        screen.contains("main · 0s · 3 files"),
        "the card should show branch, age and file count:\n{screen}"
    );
}

#[test]
fn the_panel_lists_the_files_of_the_focused_stash() {
    let dir = TempDir::new().unwrap();
    stash_repo(dir.path(), "fix status refresh race");
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('S'));
    settle(&mut app);
    let screen = dump(&draw(&mut app, 120, 30));

    assert!(
        screen.contains("Files (3)"),
        "the focused entry's files should be listed:\n{screen}"
    );
    assert!(
        screen.contains("app.txt") && screen.contains("readme.md"),
        "tracked files should be listed:\n{screen}"
    );
    assert!(
        screen.contains("notes.txt"),
        "a file that was untracked when stashed should be listed too:\n{screen}"
    );
}

#[test]
fn the_visible_keys_follow_the_focus() {
    let dir = TempDir::new().unwrap();
    stash_repo(dir.path(), "fix status refresh race");
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('S'));
    settle(&mut app);
    let entries = dump(&draw(&mut app, 120, 30));
    press(&mut app, &mut input, KeyCode::Tab);
    let files = dump(&draw(&mut app, 120, 30));

    assert!(
        entries.contains("[b] branch") && !entries.contains("[x] restore one"),
        "the entry list should offer the entry keys:\n{entries}"
    );
    assert!(
        files.contains("[x] restore one"),
        "standing on the files should offer restoring one:\n{files}"
    );
}

#[test]
fn popping_a_stash_restores_the_changes_and_removes_it() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('s'));
    assert!(!app.has_wip());
    press(&mut app, &mut input, KeyCode::Char('S'));
    press(&mut app, &mut input, KeyCode::Char('p'));

    assert!(app.has_wip(), "popping should restore the working changes");
    assert!(
        app.stash_panel()
            .is_some_and(|panel| panel.entries.is_empty()),
        "a popped stash should be gone from the list"
    );
}

#[test]
fn applying_a_stash_restores_the_changes_but_keeps_it() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('s'));
    press(&mut app, &mut input, KeyCode::Char('S'));
    press(&mut app, &mut input, KeyCode::Char('a'));

    assert!(app.has_wip(), "applying should restore the working changes");
    assert!(
        app.stash_panel()
            .is_some_and(|panel| panel.entries.len() == 1),
        "an applied stash should remain in the list"
    );
}

#[test]
fn dropping_a_stash_removes_it_after_confirmation() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('s'));
    press(&mut app, &mut input, KeyCode::Char('S'));
    press(&mut app, &mut input, KeyCode::Char('d'));
    assert!(app.confirm().is_some(), "drop should ask for confirmation");
    press(&mut app, &mut input, KeyCode::Char('y'));

    assert!(
        app.stash_panel()
            .is_some_and(|panel| panel.entries.is_empty()),
        "the dropped stash should be gone"
    );
    assert!(!app.has_wip(), "dropping does not restore the changes");
}

#[test]
fn a_conflicting_pop_surfaces_the_error_and_refreshes_the_working_view() {
    let dir = TempDir::new().unwrap();
    git_run(dir.path(), &["init", "-q", "-b", "main"]);
    git_run(dir.path(), &["config", "user.email", "demo@ravix.dev"]);
    git_run(dir.path(), &["config", "user.name", "Demo"]);
    std::fs::write(dir.path().join("f.txt"), "base\n").unwrap();
    git_run(dir.path(), &["add", "-A"]);
    git_run(dir.path(), &["commit", "-q", "-m", "base"]);
    std::fs::write(dir.path().join("f.txt"), "stashed\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('s'));
    assert!(!app.has_wip());
    std::fs::write(dir.path().join("f.txt"), "conflicting\n").unwrap();

    press(&mut app, &mut input, KeyCode::Char('S'));
    press(&mut app, &mut input, KeyCode::Char('p'));

    assert!(
        app.notice_is_error(),
        "a conflicting pop should surface an error notice"
    );
    assert!(
        app.has_wip(),
        "the working view should refresh to show the conflicted file"
    );
}

#[test]
fn stashing_a_clean_tree_reports_nothing_to_stash() {
    let dir = TempDir::new().unwrap();
    merge_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    assert!(!app.has_wip(), "the fixture tree is clean");

    press(&mut app, &mut input, KeyCode::Char('s'));

    assert_eq!(app.notice(), Some("Nothing to stash"));
}

#[test]
fn undo_pops_a_stash_save_back() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('s'));
    assert!(!app.has_wip());

    press(&mut app, &mut input, KeyCode::Char('u'));

    assert!(app.has_wip(), "undo should pop the saved stash back");
}

fn press_ctrl(app: &mut App, input: &mut InputMap, code: KeyCode) {
    let event = KeyEvent {
        code,
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    };
    if let Some(action) = input.on_key(event, app.input_context()) {
        app.update(action);
    }
}

fn type_text(app: &mut App, input: &mut InputMap, text: &str) {
    for character in text.chars() {
        press(app, input, KeyCode::Char(character));
    }
}

#[test]
fn ctrl_p_opens_the_command_palette() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press_ctrl(&mut app, &mut input, KeyCode::Char('p'));

    assert!(app.palette().is_some(), "Ctrl+P should open the palette");
    let screen = dump(&draw(&mut app, 100, 24));
    assert!(
        screen.contains("Command palette"),
        "palette title missing:\n{screen}"
    );
    assert!(
        screen.contains("Fetch") && screen.contains("Push"),
        "the palette should list the commands:\n{screen}"
    );
}

#[test]
fn typing_fuzzy_filters_the_palette() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press_ctrl(&mut app, &mut input, KeyCode::Char('p'));
    type_text(&mut app, &mut input, "push");

    let rows = app.palette().expect("palette open").rows();
    assert_eq!(rows.len(), 1, "the query should filter to a single command");
    assert_eq!(rows[0].0, "Push");
}

#[test]
fn typing_in_the_palette_selects_the_best_match() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press_ctrl(&mut app, &mut input, KeyCode::Char('p'));
    press(&mut app, &mut input, KeyCode::Down);
    press(&mut app, &mut input, KeyCode::Down);
    type_text(&mut app, &mut input, "u");

    let palette = app.palette().expect("palette open");
    assert_eq!(
        palette.selected, 0,
        "typing snaps the selection back to the top of the matches"
    );
    assert_eq!(
        palette.rows()[palette.selected].0,
        "Undo",
        "the top match is selected instead of a stale row index"
    );

    press(&mut app, &mut input, KeyCode::Enter);
    assert_eq!(
        app.notice(),
        Some("Nothing to undo"),
        "Enter runs the highlighted command, not the one the stale index pointed at"
    );
}

#[test]
fn running_a_command_dispatches_its_action_and_closes_the_palette() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press_ctrl(&mut app, &mut input, KeyCode::Char('p'));
    type_text(&mut app, &mut input, "undo");
    press(&mut app, &mut input, KeyCode::Enter);

    assert!(
        app.palette().is_none(),
        "running a command closes the palette"
    );
    assert_eq!(
        app.notice(),
        Some("Nothing to undo"),
        "the command's action should run with its usual effect"
    );
}

#[test]
fn running_a_command_that_opens_a_panel_opens_it() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press_ctrl(&mut app, &mut input, KeyCode::Char('p'));
    type_text(&mut app, &mut input, "branches");
    press(&mut app, &mut input, KeyCode::Enter);

    assert!(app.palette().is_none());
    assert!(
        app.branch_panel().is_some(),
        "running Branches from the palette should open the branch panel"
    );
}

#[test]
fn ctrl_n_and_ctrl_p_move_the_palette_focus() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press_ctrl(&mut app, &mut input, KeyCode::Char('p'));
    assert_eq!(app.palette().unwrap().selected, 0);

    press_ctrl(&mut app, &mut input, KeyCode::Char('n'));
    assert_eq!(app.palette().unwrap().selected, 1);
    press_ctrl(&mut app, &mut input, KeyCode::Char('p'));
    assert_eq!(app.palette().unwrap().selected, 0);
}

#[test]
fn n_from_the_graph_opens_the_new_branch_prompt() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('n'));

    assert!(
        app.branch_create().is_some(),
        "n from the graph should open the new-branch prompt"
    );
}

#[test]
fn esc_closes_the_palette_without_running_anything() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press_ctrl(&mut app, &mut input, KeyCode::Char('p'));
    press(&mut app, &mut input, KeyCode::Esc);

    assert!(app.palette().is_none());
    assert!(app.notice().is_none(), "Esc should not run anything");
}

#[test]
fn arrows_move_the_palette_focus() {
    let dir = TempDir::new().unwrap();
    fixture_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press_ctrl(&mut app, &mut input, KeyCode::Char('p'));
    assert_eq!(app.palette().unwrap().selected, 0);

    press(&mut app, &mut input, KeyCode::Down);
    assert_eq!(app.palette().unwrap().selected, 1);
    press(&mut app, &mut input, KeyCode::Up);
    assert_eq!(app.palette().unwrap().selected, 0);
}

fn git_ok(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .success()
}

fn rebase_conflict_repo(dir: &Path) {
    let base = "L1\nx1\nx2\nx3\nx4\nx5\nL2\n";
    init_with_base(dir, "f.txt", base);
    git_run(dir, &["switch", "-qc", "target"]);
    commit_file(dir, "f.txt", "T1\nx1\nx2\nx3\nx4\nx5\nT2\n", "target edit");
    git_run(dir, &["switch", "-q", "main"]);
    commit_file(dir, "f.txt", "M1\nx1\nx2\nx3\nx4\nx5\nL2\n", "main c1");
    commit_file(dir, "f.txt", "M1\nx1\nx2\nx3\nx4\nx5\nM2\n", "main c2");
}

fn clean_rebase_repo(dir: &Path) {
    init_with_base(dir, "a.txt", "a\n");
    git_run(dir, &["switch", "-qc", "target"]);
    commit_file(dir, "t.txt", "t\n", "target work");
    git_run(dir, &["switch", "-q", "main"]);
    commit_file(dir, "m.txt", "m\n", "main work");
}

fn open_rebase_onto_target(app: &mut App, input: &mut InputMap) {
    open_join_on_feature(app, input);
    press(app, input, KeyCode::Char('j'));
    press(app, input, KeyCode::Char('j'));
    press(app, input, KeyCode::Char('j'));
    press(app, input, KeyCode::Enter);
}

#[test]
fn a_multi_step_rebase_resolves_through_the_browser() {
    let dir = TempDir::new().unwrap();
    rebase_conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_rebase_onto_target(&mut app, &mut input);

    assert!(
        app.conflict_browser()
            .is_some_and(|b| b.op == ravix::conflict::OpKind::Rebase),
        "a conflicting rebase should open the browser as a rebase"
    );
    let screen = dump(&draw(&mut app, 100, 28));
    assert!(
        screen.contains("rebase in progress") && screen.contains("step 1/2"),
        "the banner should show rebase progress:\n{screen}"
    );

    press(&mut app, &mut input, KeyCode::Char('t'));
    press(&mut app, &mut input, KeyCode::Char('c'));

    assert!(
        app.conflict_browser().is_some(),
        "the next conflicting commit should re-open the browser"
    );
    let screen = dump(&draw(&mut app, 100, 28));
    assert!(
        screen.contains("step 2/2"),
        "the banner should advance to the next step:\n{screen}"
    );

    press(&mut app, &mut input, KeyCode::Char('t'));
    press(&mut app, &mut input, KeyCode::Char('c'));

    assert!(
        app.conflict_browser().is_none(),
        "the rebase should be complete"
    );
    assert_eq!(app.meta().head_branch.as_deref(), Some("main"));
    assert!(
        git_ok(
            dir.path(),
            &["merge-base", "--is-ancestor", "target", "main"]
        ),
        "main should now sit on top of target"
    );
}

#[test]
fn a_clean_rebase_replays_without_the_browser() {
    let dir = TempDir::new().unwrap();
    clean_rebase_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_rebase_onto_target(&mut app, &mut input);

    assert!(
        app.conflict_browser().is_none(),
        "a clean rebase should not open the browser"
    );
    assert!(
        git_ok(
            dir.path(),
            &["merge-base", "--is-ancestor", "target", "main"]
        ),
        "main should have been replayed onto target"
    );
}

#[test]
fn undo_reverts_a_clean_rebase() {
    let dir = TempDir::new().unwrap();
    clean_rebase_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_rebase_onto_target(&mut app, &mut input);
    assert!(
        app.conflict_browser().is_none(),
        "the rebase should be clean"
    );
    assert!(git_ok(
        dir.path(),
        &["merge-base", "--is-ancestor", "target", "main"]
    ));

    press(&mut app, &mut input, KeyCode::Char('u'));

    assert!(
        !git_ok(
            dir.path(),
            &["merge-base", "--is-ancestor", "target", "main"]
        ),
        "undo should take a clean rebase back off target"
    );
    assert_eq!(
        app.head_commit().expect("HEAD commit").summary,
        "main work",
        "undo should restore the pre-rebase tip"
    );
}

#[test]
fn aborting_a_rebase_returns_to_the_pre_rebase_tip() {
    let dir = TempDir::new().unwrap();
    rebase_conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_rebase_onto_target(&mut app, &mut input);
    assert!(app.conflict_browser().is_some());

    press(&mut app, &mut input, KeyCode::Esc);

    assert!(app.conflict_browser().is_none());
    assert_eq!(
        app.head_commit().expect("HEAD commit").summary,
        "main c2",
        "abort should restore the pre-rebase tip"
    );
    assert!(
        !git_ok(
            dir.path(),
            &["merge-base", "--is-ancestor", "target", "main"]
        ),
        "the rebase should have been undone"
    );
}

#[test]
fn undo_reverts_a_completed_rebase() {
    let dir = TempDir::new().unwrap();
    rebase_conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_rebase_onto_target(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('t'));
    press(&mut app, &mut input, KeyCode::Char('c'));
    press(&mut app, &mut input, KeyCode::Char('t'));
    press(&mut app, &mut input, KeyCode::Char('c'));
    assert!(app.conflict_browser().is_none());
    assert!(git_ok(
        dir.path(),
        &["merge-base", "--is-ancestor", "target", "main"]
    ));

    press(&mut app, &mut input, KeyCode::Char('u'));

    assert_eq!(
        app.head_commit().expect("HEAD commit").summary,
        "main c2",
        "undo should restore the pre-rebase tip"
    );
    assert!(
        !git_ok(
            dir.path(),
            &["merge-base", "--is-ancestor", "target", "main"]
        ),
        "undo should take main back off target"
    );
}

#[test]
fn skipping_a_commit_drops_it_from_the_rebase() {
    let dir = TempDir::new().unwrap();
    rebase_conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_rebase_onto_target(&mut app, &mut input);
    assert!(app.conflict_browser().is_some());

    press(&mut app, &mut input, KeyCode::Char('s'));
    assert!(
        app.conflict_browser().is_some(),
        "skipping the first commit should advance to the next conflicting one"
    );

    press(&mut app, &mut input, KeyCode::Char('t'));
    press(&mut app, &mut input, KeyCode::Char('c'));

    assert!(app.conflict_browser().is_none());
    let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
    assert!(
        content.contains("T1") && !content.contains("M1"),
        "the skipped commit's change should be absent:\n{content}"
    );
}

#[test]
fn an_in_progress_rebase_is_detected_when_ravix_opens() {
    let dir = TempDir::new().unwrap();
    rebase_conflict_repo(dir.path());
    let out = Command::new("git")
        .current_dir(dir.path())
        .args(["-c", "core.editor=true", "rebase", "target"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "the rebase should have conflicted");

    let mut app = App::open(dir.path()).unwrap();

    assert!(
        app.conflict_browser()
            .is_some_and(|b| b.op == ravix::conflict::OpKind::Rebase),
        "an already-conflicted rebase should open the browser on startup"
    );
    let screen = dump(&draw(&mut app, 100, 28));
    assert!(
        screen.contains("rebase in progress"),
        "the banner should show the rebase:\n{screen}"
    );
}

fn code_repo(dir: &Path) {
    let run = |args: &[&str]| git_run(dir, args);

    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "demo@ravix.dev"]);
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
fn undo_restores_a_discarded_file() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('d'));
    press(&mut app, &mut input, KeyCode::Char('y'));
    assert!(
        !app.status().unstaged.iter().any(|f| f.path == "app.txt"),
        "app.txt should have been discarded"
    );

    press(&mut app, &mut input, KeyCode::Char('u'));
    let screen = dump(&draw(&mut app, 100, 28));

    let content = std::fs::read_to_string(dir.path().join("app.txt")).unwrap();
    assert!(
        content.contains("LINE ONE"),
        "the discarded edit should be restored:\n{content}"
    );
    assert!(
        app.status().unstaged.iter().any(|f| f.path == "app.txt"),
        "app.txt should be back among unstaged changes"
    );
    assert!(
        screen.contains("Undid"),
        "an undo notice should show:\n{screen}"
    );
}

#[test]
fn undo_reverts_a_commit_and_restages_its_changes() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('c'));
    for character in "commit it".chars() {
        press(&mut app, &mut input, KeyCode::Char(character));
    }
    press(&mut app, &mut input, KeyCode::Enter);
    assert_eq!(app.commits()[0].summary, "commit it");

    press(&mut app, &mut input, KeyCode::Char('u'));

    assert_eq!(
        app.commits()[0].summary,
        "initial commit",
        "the commit should be undone"
    );
    assert!(
        app.status().staged.iter().any(|f| f.path == "readme.md"),
        "the committed change should return staged: {:?}",
        app.status()
    );
}

#[test]
fn undo_reverts_a_stage() {
    let dir = TempDir::new().unwrap();
    dirty_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char(' '));
    assert!(app.status().staged.iter().any(|f| f.path == "app.txt"));

    press(&mut app, &mut input, KeyCode::Char('u'));

    assert!(
        !app.status().staged.iter().any(|f| f.path == "app.txt"),
        "the stage should be undone: {:?}",
        app.status()
    );
    assert!(
        app.status().unstaged.iter().any(|f| f.path == "app.txt"),
        "app.txt should be unstaged again"
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
