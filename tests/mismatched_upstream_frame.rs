mod common;

use std::path::Path;
use std::time::Duration;

use crossterm::event::KeyCode;
use tempfile::TempDir;

use common::{draw, dump, git, init_repo, press};
use ravix::app::{Action, App};
use ravix::event::InputMap;

fn settle(app: &mut App) {
    for _ in 0..24 {
        app.update(Action::Tick(Duration::from_millis(16)));
    }
}

fn commit_on(dir: &Path, file: &str, message: &str) {
    std::fs::write(dir.join(file), message).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", message]);
}

fn mismatched_repo(work: &Path, remote: &Path) {
    git(remote, &["init", "-q", "--bare", "-b", "staging"]);
    init_repo(work);
    commit_on(work, "README", "Root");
    git(work, &["branch", "-qM", "staging"]);
    git(work, &["remote", "add", "origin", remote.to_str().unwrap()]);
    git(work, &["push", "-q", "-u", "origin", "staging"]);
    git(
        work,
        &["checkout", "-qb", "release/x", "--track", "origin/staging"],
    );
    commit_on(work, "release.txt", "Release work");
}

fn panel_row<'a>(screen: &'a str, needle: &str) -> &'a str {
    screen
        .lines()
        .filter_map(|line| line.rsplit('│').nth(1))
        .find(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("no panel row with {needle:?}:\n{screen}"))
}

#[test]
fn the_header_marks_a_mismatched_upstream() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    mismatched_repo(work.path(), remote.path());
    let mut app = App::open(work.path()).unwrap();

    let screen = dump(&draw(&mut app, 120, 24));

    assert!(
        screen.contains("↑1 ↓0  → origin/staging"),
        "the header should name the mismatched upstream:\n{screen}"
    );
}

#[test]
fn the_branch_panel_marks_only_the_mismatched_branch() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    mismatched_repo(work.path(), remote.path());
    git(work.path(), &["switch", "-q", "staging"]);
    commit_on(work.path(), "staging.txt", "Staging work");
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    let screen = dump(&draw(&mut app, 120, 24));

    let release = panel_row(&screen, "release/x");
    assert!(
        release.contains("↑1 ↓0  → origin/staging"),
        "the mismatched branch carries the mark:\n{screen}"
    );
    let staging = panel_row(&screen, "staging  ↑");
    assert!(
        staging.contains("↑1 ↓0") && !staging.contains("→"),
        "a matching upstream shows no mark:\n{screen}"
    );
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

fn upstream_of(dir: &Path, branch: &str) -> String {
    git(
        dir,
        &[
            "rev-parse",
            "--abbrev-ref",
            &format!("{branch}@{{upstream}}"),
        ],
    )
    .trim()
    .to_string()
}

#[test]
fn pushing_a_mismatched_branch_opens_the_push_chooser_on_relink() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    mismatched_repo(work.path(), remote.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('P'));

    assert!(
        !app.remote_pending(),
        "a mismatched upstream must not push blindly"
    );
    let screen = dump(&draw(&mut app, 120, 24));
    let relink = screen
        .lines()
        .find(|line| line.contains("Push to origin/release/x and relink"))
        .unwrap_or_else(|| panic!("the chooser should offer relinking:\n{screen}"));
    assert!(relink.contains('❯'), "relink is focused on open:\n{screen}");
    assert!(
        screen.contains("Push into origin/staging"),
        "the chooser should offer pushing into the upstream:\n{screen}"
    );
}

#[test]
fn relinking_publishes_the_branch_under_its_own_name() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    mismatched_repo(work.path(), remote.path());
    let staging_before = git(remote.path(), &["rev-parse", "staging"]);
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('P'));
    press(&mut app, &mut input, KeyCode::Enter);
    drive_remote(&mut app);

    assert_eq!(app.notice(), Some("Pushed"));
    assert_eq!(
        git(remote.path(), &["rev-parse", "release/x"]),
        git(work.path(), &["rev-parse", "release/x"]),
        "the branch lands on the remote under its own name"
    );
    assert_eq!(
        git(remote.path(), &["rev-parse", "staging"]),
        staging_before,
        "relinking never touches the old upstream"
    );
    assert_eq!(upstream_of(work.path(), "release/x"), "origin/release/x");
    let screen = dump(&draw(&mut app, 120, 24));
    assert!(
        !screen.contains("→ origin/staging"),
        "after relinking the mark disappears:\n{screen}"
    );
}

#[test]
fn pushing_into_the_upstream_asks_first_with_the_commit_count() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    mismatched_repo(work.path(), remote.path());
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('P'));
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);

    assert!(
        !app.remote_pending(),
        "pushing into the upstream must ask first"
    );
    let screen = dump(&draw(&mut app, 120, 24));
    assert!(
        screen.contains("Push 1 commit(s) into origin/staging?"),
        "the confirmation states what lands where:\n{screen}"
    );

    press(&mut app, &mut input, KeyCode::Char('y'));
    drive_remote(&mut app);

    assert_eq!(app.notice(), Some("Pushed"));
    assert_eq!(
        git(remote.path(), &["rev-parse", "staging"]),
        git(work.path(), &["rev-parse", "release/x"]),
        "the upstream advances to the branch"
    );
    assert_eq!(upstream_of(work.path(), "release/x"), "origin/staging");
}

#[test]
fn a_branch_level_with_its_upstream_can_only_be_published() {
    let remote = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    mismatched_repo(work.path(), remote.path());
    git(work.path(), &["reset", "-q", "--hard", "origin/staging"]);
    let mut app = App::open(work.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('P'));
    let screen = dump(&draw(&mut app, 120, 24));
    assert!(
        screen.contains("Push to origin/release/x and relink"),
        "even at ↑0 the branch can be published:\n{screen}"
    );

    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Enter);
    assert!(app.confirm().is_none(), "a blocked choice never asks");
    assert!(!app.remote_pending(), "a blocked choice never pushes");
    assert!(
        app.notice()
            .is_some_and(|notice| notice.contains("nothing to push")),
        "the refusal explains why: {:?}",
        app.notice()
    );
}
