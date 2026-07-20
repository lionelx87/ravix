mod common;

use std::time::Duration;

use crossterm::event::KeyCode;
use tempfile::TempDir;

use ravix::app::{Action, App};
use ravix::event::InputMap;

fn panel_dump(app: &mut App) -> String {
    let buffer = common::draw(app, 120, 20);
    common::dump(&buffer)
}

#[test]
fn peek_fills_once_the_selection_settles() {
    let dir = TempDir::new().unwrap();
    common::init_repo(dir.path());
    std::fs::write(dir.path().join("first.txt"), "one").unwrap();
    common::git(dir.path(), &["add", "-A"]);
    common::git(dir.path(), &["commit", "-qm", "first"]);
    std::fs::write(dir.path().join("second.txt"), "two").unwrap();
    common::git(dir.path(), &["add", "-A"]);
    common::git(dir.path(), &["commit", "-qm", "second"]);

    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    common::press(&mut app, &mut input, KeyCode::Enter);
    app.update(Action::Tick(Duration::from_secs(2)));
    assert!(panel_dump(&mut app).contains("second.txt"));

    common::press(&mut app, &mut input, KeyCode::Char('j'));
    assert!(!panel_dump(&mut app).contains("first.txt"));
    app.update(Action::Tick(Duration::from_millis(200)));
    assert!(panel_dump(&mut app).contains("first.txt"));
}

#[test]
fn revisiting_a_cached_commit_fills_the_peek_immediately() {
    let dir = TempDir::new().unwrap();
    common::init_repo(dir.path());
    std::fs::write(dir.path().join("first.txt"), "one").unwrap();
    common::git(dir.path(), &["add", "-A"]);
    common::git(dir.path(), &["commit", "-qm", "first"]);
    std::fs::write(dir.path().join("second.txt"), "two").unwrap();
    common::git(dir.path(), &["add", "-A"]);
    common::git(dir.path(), &["commit", "-qm", "second"]);

    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    common::press(&mut app, &mut input, KeyCode::Enter);
    app.update(Action::Tick(Duration::from_secs(2)));
    common::press(&mut app, &mut input, KeyCode::Char('j'));
    app.update(Action::Tick(Duration::from_millis(200)));
    common::press(&mut app, &mut input, KeyCode::Char('k'));
    assert!(panel_dump(&mut app).contains("second.txt"));
}
