mod common;

use std::path::Path;
use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::style::Style;
use tempfile::TempDir;

use common::{draw, dump, git, init_repo, press};
use ravix::app::{Action, App};
use ravix::event::InputMap;

fn settle(app: &mut App) {
    for _ in 0..40 {
        app.update(Action::Tick(Duration::from_millis(16)));
    }
}

fn commit_on(dir: &Path, file: &str, message: &str) {
    std::fs::write(dir.join(file), message).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", message]);
}

fn copy_repo(dir: &Path) {
    init_repo(dir);
    commit_on(dir, "README", "Root");
    commit_on(dir, "middle.rs", "middle work");
    commit_on(dir, "main.rs", "main tip");
    git(
        dir,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/repo.git",
        ],
    );
    git(dir, &["update-ref", "refs/remotes/origin/main", "main"]);
    git(dir, &["branch", "-q", "-u", "origin/main"]);
    git(dir, &["checkout", "-qb", "feature/x", "HEAD~2"]);
    commit_on(dir, "x.rs", "x work");
    git(dir, &["checkout", "-q", "main"]);
}

fn short(dir: &Path, rev: &str) -> String {
    git(dir, &["rev-parse", "--short", rev]).trim().to_string()
}

fn screen(app: &mut App) -> String {
    dump(&draw(app, 120, 30))
}

fn styles_of(buffer: &Buffer, text: &str) -> Vec<Style> {
    let area = buffer.area();
    let mut styles = Vec::new();
    for y in 0..area.height {
        let row: String = (0..area.width)
            .map(|x| buffer.cell((x, y)).unwrap().symbol().to_string())
            .collect();
        for (byte, _) in row.match_indices(text) {
            let x = row[..byte].chars().count() as u16;
            styles.push(buffer.cell((x, y)).unwrap().style());
        }
    }
    assert!(!styles.is_empty(), "{text} is not on screen");
    styles
}

fn flashes(app: &mut App, text: &str, copy: impl FnOnce(&mut App)) {
    flashes_at(app, text, |styles| styles[0], copy);
}

fn flashes_at(
    app: &mut App,
    text: &str,
    pick: impl Fn(&[Style]) -> Style,
    copy: impl FnOnce(&mut App),
) {
    let before = pick(&styles_of(&draw(app, 120, 30), text));
    copy(app);
    let during = pick(&styles_of(&draw(app, 120, 30), text));
    settle(app);
    let after = pick(&styles_of(&draw(app, 120, 30), text));
    assert_ne!(during, before, "{text} flashes right after the copy");
    assert_eq!(after, before, "the flash on {text} fades out");
}

fn select_in_graph(app: &mut App, input: &mut InputMap, summary: &str) {
    press(app, input, KeyCode::Char('g'));
    press(app, input, KeyCode::Char('g'));
    for _ in 0..app.commits().len() {
        if app
            .selected_commit()
            .is_some_and(|commit| commit.summary == summary)
        {
            return;
        }
        press(app, input, KeyCode::Char('j'));
    }
    panic!("no commit with summary {summary}");
}

fn open(dir: &Path) -> (App, InputMap) {
    let mut app = App::open(dir).unwrap();
    settle(&mut app);
    (app, InputMap::default())
}

#[test]
fn y_in_the_graph_copies_the_short_hash_of_the_selected_commit() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    select_in_graph(&mut app, &mut input, "middle work");

    press(&mut app, &mut input, KeyCode::Char('y'));

    let middle = short(dir.path(), "main~1");
    assert_eq!(app.take_clipboard().as_deref(), Some(middle.as_str()));
    let view = screen(&mut app);
    assert!(
        view.contains(&format!("Copied {middle} to the clipboard")),
        "a notice names the copied hash:\n{view}"
    );
}

#[test]
fn y_in_the_fullscreen_commit_diff_copies_that_commits_short_hash() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    select_in_graph(&mut app, &mut input, "middle work");
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);

    press(&mut app, &mut input, KeyCode::Char('y'));

    let middle = short(dir.path(), "main~1");
    assert_eq!(app.take_clipboard().as_deref(), Some(middle.as_str()));
}

#[test]
fn shift_y_in_the_branch_panel_copies_the_focused_branch_name() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));

    press(&mut app, &mut input, KeyCode::Char('Y'));

    assert_eq!(app.take_clipboard().as_deref(), Some("feature/x"));
    let view = screen(&mut app);
    assert!(
        view.contains("Copied feature/x to the clipboard"),
        "a notice names the copied branch:\n{view}"
    );
}

#[test]
fn y_in_the_branch_panel_copies_nothing() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);

    press(&mut app, &mut input, KeyCode::Char('y'));

    assert_eq!(app.take_clipboard(), None);
}

#[test]
fn shift_y_in_the_pin_switcher_copies_the_selected_pin_and_keeps_it_open() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('p'));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('B'));

    press(&mut app, &mut input, KeyCode::Char('Y'));

    assert_eq!(app.take_clipboard().as_deref(), Some("feature/x"));
    assert!(app.pin_switcher().is_some(), "copying is not a checkout");
}

#[test]
fn shift_y_in_branch_review_copies_the_reviewed_branch() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('l'));
    settle(&mut app);

    press(&mut app, &mut input, KeyCode::Char('Y'));

    assert_eq!(app.take_clipboard().as_deref(), Some("feature/x"));
}

#[test]
fn shift_y_on_a_commit_with_one_branch_copies_it() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    select_in_graph(&mut app, &mut input, "x work");

    press(&mut app, &mut input, KeyCode::Char('Y'));

    assert_eq!(app.take_clipboard().as_deref(), Some("feature/x"));
}

#[test]
fn shift_y_on_a_commit_without_branches_says_so_and_copies_nothing() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    select_in_graph(&mut app, &mut input, "middle work");

    press(&mut app, &mut input, KeyCode::Char('Y'));

    assert_eq!(app.take_clipboard(), None);
    let view = screen(&mut app);
    assert!(
        view.contains("No branch on this commit"),
        "a notice explains why nothing was copied:\n{view}"
    );
}

#[test]
fn shift_y_on_a_commit_with_several_branches_asks_which_one() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    select_in_graph(&mut app, &mut input, "main tip");

    press(&mut app, &mut input, KeyCode::Char('Y'));

    assert_eq!(app.take_clipboard(), None);
    let chooser = screen(&mut app);
    assert!(
        chooser.contains("1 main") && chooser.contains("2 origin/main"),
        "the chooser lists the current branch first:\n{chooser}"
    );

    press(&mut app, &mut input, KeyCode::Char('2'));

    assert_eq!(app.take_clipboard().as_deref(), Some("origin/main"));
    let after = screen(&mut app);
    assert!(
        !after.contains("2 origin/main"),
        "copying closes the chooser:\n{after}"
    );
}

#[test]
fn enter_in_the_copy_chooser_copies_the_highlighted_branch() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    select_in_graph(&mut app, &mut input, "main tip");
    press(&mut app, &mut input, KeyCode::Char('Y'));
    press(&mut app, &mut input, KeyCode::Char('j'));

    press(&mut app, &mut input, KeyCode::Enter);

    assert_eq!(app.take_clipboard().as_deref(), Some("origin/main"));
}

#[test]
fn esc_closes_the_copy_chooser_without_copying() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    select_in_graph(&mut app, &mut input, "main tip");
    press(&mut app, &mut input, KeyCode::Char('Y'));

    press(&mut app, &mut input, KeyCode::Esc);

    assert_eq!(app.take_clipboard(), None);
    assert_eq!(app.input_context(), ravix::app::InputContext::Graph);
}

#[test]
fn copying_a_hash_in_the_graph_flashes_it() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    select_in_graph(&mut app, &mut input, "middle work");
    let middle = short(dir.path(), "main~1");

    flashes(&mut app, &middle, |app| {
        press(app, &mut input, KeyCode::Char('y'))
    });
}

#[test]
fn copying_a_branch_in_the_graph_flashes_its_label() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    select_in_graph(&mut app, &mut input, "x work");

    flashes(&mut app, "feature/x", |app| {
        press(app, &mut input, KeyCode::Char('Y'))
    });
}

fn second(styles: &[Style]) -> Style {
    styles[1]
}

#[test]
fn copying_in_the_branch_panel_flashes_the_branch_row() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));

    flashes_at(&mut app, "feature/x", second, |app| {
        press(app, &mut input, KeyCode::Char('Y'))
    });
}

#[test]
fn copying_in_the_pin_switcher_flashes_the_pin() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('p'));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('B'));

    flashes_at(&mut app, "feature/x", second, |app| {
        press(app, &mut input, KeyCode::Char('Y'))
    });
}

fn open_review(app: &mut App, input: &mut InputMap) {
    press(app, input, KeyCode::Char('b'));
    settle(app);
    press(app, input, KeyCode::Char('j'));
    press(app, input, KeyCode::Char('l'));
    settle(app);
}

#[test]
fn copying_a_hash_in_branch_review_flashes_the_commit_row() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    open_review(&mut app, &mut input);
    let tip = short(dir.path(), "feature/x");

    flashes_at(&mut app, &tip, second, |app| {
        press(app, &mut input, KeyCode::Char('y'))
    });
}

#[test]
fn copying_the_branch_in_branch_review_flashes_it_in_the_band() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    open_review(&mut app, &mut input);

    flashes(&mut app, "feature/x", |app| {
        press(app, &mut input, KeyCode::Char('Y'))
    });
}

#[test]
fn copying_in_the_fullscreen_commit_diff_flashes_the_heading_hash() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    select_in_graph(&mut app, &mut input, "middle work");
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    let middle = short(dir.path(), "main~1");

    flashes(&mut app, &middle, |app| {
        press(app, &mut input, KeyCode::Char('y'))
    });
}

#[test]
fn y_in_the_pin_switcher_copies_nothing() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    let (mut app, mut input) = open(dir.path());
    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('p'));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('B'));

    press(&mut app, &mut input, KeyCode::Char('y'));

    assert_eq!(app.take_clipboard(), None);
}

#[test]
fn choosing_a_branch_that_is_not_the_first_pill_flashes_its_own_label() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    git(dir.path(), &["branch", "other", "feature/x"]);
    let (mut app, mut input) = open(dir.path());
    select_in_graph(&mut app, &mut input, "x work");
    press(&mut app, &mut input, KeyCode::Char('Y'));

    flashes(&mut app, "other", |app| {
        press(app, &mut input, KeyCode::Char('2'))
    });
}

#[test]
fn copying_a_branch_that_only_ends_like_a_label_does_not_flash_it() {
    let dir = TempDir::new().unwrap();
    copy_repo(dir.path());
    git(dir.path(), &["branch", "fix/main", "main~2"]);
    let (mut app, mut input) = open(dir.path());
    select_in_graph(&mut app, &mut input, "Root");
    let before = styles_of(&draw(&mut app, 120, 30), "✓ main")[0];

    press(&mut app, &mut input, KeyCode::Char('Y'));

    assert_eq!(app.take_clipboard().as_deref(), Some("fix/main"));
    let during = styles_of(&draw(&mut app, 120, 30), "✓ main")[0];
    assert_eq!(during, before, "main's pill stays still");
}
