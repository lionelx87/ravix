mod common;

use std::path::Path;

use crossterm::event::KeyCode;
use tempfile::TempDir;

use common::{draw, dump, git, init_repo, press};
use ravix::app::App;
use ravix::conflict::OpKind;
use ravix::event::InputMap;

fn stash_conflict_repo(dir: &Path) {
    init_repo(dir);
    std::fs::write(dir.join("f.txt"), "a\nb\nc\n").unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", "base"]);
    std::fs::write(dir.join("f.txt"), "a\nSTASHED\nc\n").unwrap();
    git(dir, &["stash", "push", "-q", "-m", "wip"]);
    std::fs::write(dir.join("f.txt"), "a\nHEAD\nc\n").unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", "diverge"]);
}

fn apply_conflicting_stash(dir: &Path) {
    let status = std::process::Command::new("git")
        .current_dir(dir)
        .args(["stash", "apply", "stash@{0}"])
        .output()
        .unwrap()
        .status;
    assert!(!status.success(), "the fixture apply should conflict");
}

fn stash_count(dir: &Path) -> usize {
    git(dir, &["stash", "list"]).lines().count()
}

fn open_stash_panel(app: &mut App, input: &mut InputMap) {
    press(app, input, KeyCode::Char('S'));
}

#[test]
fn a_conflicting_apply_opens_the_resolver() {
    let dir = TempDir::new().unwrap();
    stash_conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_stash_panel(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('a'));

    assert!(
        app.conflict_browser()
            .is_some_and(|b| b.op == OpKind::Stash),
        "a conflicting apply should open the browser as a stash"
    );
    assert!(
        app.notice().is_some_and(|text| text.contains("conflict")),
        "the failure should say what happened: {:?}",
        app.notice()
    );
    assert!(
        app.stash_panel().is_none(),
        "the stash panel should step aside instead of holding the keys"
    );
    let screen = dump(&draw(&mut app, 100, 28));
    assert!(
        screen.contains("stash conflicts") && screen.contains("f.txt"),
        "the browser should name the conflicted file:\n{screen}"
    );
}

#[test]
fn resolving_a_conflicting_apply_keeps_the_stash() {
    let dir = TempDir::new().unwrap();
    stash_conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_stash_panel(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('a'));
    press(&mut app, &mut input, KeyCode::Char('t'));
    press(&mut app, &mut input, KeyCode::Char('c'));

    assert!(app.conflict_browser().is_none(), "the browser should close");
    let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
    assert_eq!(content, "a\nSTASHED\nc\n", "theirs should win");
    assert_eq!(stash_count(dir.path()), 1, "an applied stash is kept");
}

#[test]
fn resolving_a_conflicting_pop_drops_the_stash() {
    let dir = TempDir::new().unwrap();
    stash_conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_stash_panel(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('p'));
    press(&mut app, &mut input, KeyCode::Char('t'));
    press(&mut app, &mut input, KeyCode::Char('c'));

    assert!(app.conflict_browser().is_none(), "the browser should close");
    assert_eq!(stash_count(dir.path()), 0, "a popped stash is dropped");
}

#[test]
fn discarding_a_conflicting_apply_restores_the_files() {
    let dir = TempDir::new().unwrap();
    stash_conflict_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_stash_panel(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('a'));
    press(&mut app, &mut input, KeyCode::Char('A'));

    assert!(
        app.confirm().is_some(),
        "discarding should ask before touching the files"
    );

    press(&mut app, &mut input, KeyCode::Char('y'));

    assert!(app.conflict_browser().is_none(), "the browser should close");
    let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
    assert_eq!(content, "a\nHEAD\nc\n", "the file should be back at HEAD");
    assert_eq!(
        git(dir.path(), &["status", "--porcelain"]),
        "",
        "discarding should leave a clean tree"
    );
    assert_eq!(stash_count(dir.path()), 1, "the stash survives a discard");
}

#[test]
fn a_stash_conflict_left_behind_is_detected_when_ravix_opens() {
    let dir = TempDir::new().unwrap();
    stash_conflict_repo(dir.path());
    apply_conflicting_stash(dir.path());

    let mut app = App::open(dir.path()).unwrap();

    assert!(
        app.conflict_browser()
            .is_some_and(|b| b.op == OpKind::Stash),
        "a leftover stash conflict should open the browser"
    );
    let screen = dump(&draw(&mut app, 100, 28));
    assert!(
        screen.contains("f.txt"),
        "the conflicted file should be listed:\n{screen}"
    );
}

#[test]
fn a_conflicted_file_shows_up_in_the_working_tree() {
    let dir = TempDir::new().unwrap();
    stash_conflict_repo(dir.path());
    apply_conflicting_stash(dir.path());

    let app = App::open(dir.path()).unwrap();

    assert!(
        app.has_wip(),
        "a conflicted file should count as work in progress"
    );
}
