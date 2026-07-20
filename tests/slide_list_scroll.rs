mod common;

use std::time::Duration;

use crossterm::event::KeyCode;
use tempfile::TempDir;

use ravix::app::{Action, App};
use ravix::event::InputMap;

#[test]
fn branch_selection_beyond_panel_height_stays_visible() {
    let dir = TempDir::new().unwrap();
    common::init_repo(dir.path());
    std::fs::write(dir.path().join("file.txt"), "hi").unwrap();
    common::git(dir.path(), &["add", "-A"]);
    common::git(dir.path(), &["commit", "-qm", "init"]);
    for index in 0..30 {
        common::git(dir.path(), &["branch", &format!("b-{index:02}")]);
    }

    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    app.update(Action::ToggleBranches);
    app.update(Action::Tick(Duration::from_secs(2)));
    for _ in 0..30 {
        common::press(&mut app, &mut input, KeyCode::Char('j'));
    }

    let buffer = common::draw(&mut app, 100, 15);
    let dump = common::dump(&buffer);
    assert!(
        dump.contains("b-29"),
        "selected branch is not visible in the panel:\n{dump}"
    );
}
