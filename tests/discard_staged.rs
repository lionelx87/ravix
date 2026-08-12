mod common;

use std::path::Path;

use crossterm::event::KeyCode;
use tempfile::TempDir;

use common::{git, init_repo, press};
use ravix::app::{Action, App};
use ravix::event::InputMap;

fn base_repo(dir: &Path) {
    init_repo(dir);
    std::fs::write(dir.join("f.txt"), "a\nb\nc\n").unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", "base"]);
}

fn open_working(app: &mut App, input: &mut InputMap) {
    press(app, input, KeyCode::Char('k'));
    press(app, input, KeyCode::Enter);
    app.update(Action::Tick(std::time::Duration::from_secs(2)));
}

#[test]
fn discarding_a_staged_change_takes_the_file_back_to_head() {
    let dir = TempDir::new().unwrap();
    base_repo(dir.path());
    std::fs::write(dir.path().join("f.txt"), "a\nSTAGED\nc\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('d'));
    press(&mut app, &mut input, KeyCode::Char('y'));

    let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
    assert_eq!(content, "a\nb\nc\n", "the file should be back at HEAD");
    assert_eq!(
        git(dir.path(), &["status", "--porcelain"]),
        "",
        "discarding a staged change should also clear the index"
    );
}

#[test]
fn discarding_a_staged_addition_removes_the_file() {
    let dir = TempDir::new().unwrap();
    base_repo(dir.path());
    std::fs::write(dir.path().join("new.txt"), "brand new\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('d'));
    press(&mut app, &mut input, KeyCode::Char('y'));

    assert!(
        !dir.path().join("new.txt").exists(),
        "discarding a staged addition should remove the file"
    );
}

#[test]
fn discarding_after_a_resolved_stash_apply_reverts_the_applied_change() {
    let dir = TempDir::new().unwrap();
    base_repo(dir.path());
    std::fs::write(dir.path().join("f.txt"), "a\nSTASHED\nc\n").unwrap();
    git(dir.path(), &["stash", "push", "-q", "-m", "wip"]);
    std::fs::write(dir.path().join("f.txt"), "a\nHEAD\nc\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-qm", "diverge"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('S'));
    press(&mut app, &mut input, KeyCode::Char('a'));
    press(&mut app, &mut input, KeyCode::Char('t'));
    press(&mut app, &mut input, KeyCode::Char('c'));

    open_working(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Char('d'));
    press(&mut app, &mut input, KeyCode::Char('y'));

    let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
    assert_eq!(
        content, "a\nHEAD\nc\n",
        "discarding should undo what the stash brought in"
    );
    assert_eq!(
        git(dir.path(), &["status", "--porcelain"]),
        "",
        "the tree should be clean again"
    );
}
