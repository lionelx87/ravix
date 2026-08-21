mod common;

use std::path::Path;

use crossterm::event::KeyCode;
use tempfile::TempDir;

use common::{git, init_repo, press};
use ravix::app::{Action, App};
use ravix::event::InputMap;

fn staged_repo(dir: &Path) {
    init_repo(dir);
    std::fs::write(dir.join("one.txt"), "one\n").unwrap();
    std::fs::write(dir.join("two.txt"), "two\n").unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", "base"]);
    std::fs::write(dir.join("one.txt"), "one changed\n").unwrap();
    std::fs::write(dir.join("two.txt"), "two changed\n").unwrap();
    git(dir, &["add", "-A"]);
}

fn open_working(app: &mut App, input: &mut InputMap) {
    press(app, input, KeyCode::Char('k'));
    press(app, input, KeyCode::Enter);
    app.update(Action::Tick(std::time::Duration::from_secs(2)));
}

#[test]
fn an_external_commit_clears_the_staged_zone_without_a_restart() {
    let dir = TempDir::new().unwrap();
    staged_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_working(&mut app, &mut input);
    assert_eq!(
        app.status().staged.len(),
        2,
        "the two staged files are visible before the commit"
    );

    git(dir.path(), &["commit", "-qm", "outside"]);
    app.poll_status();

    assert!(
        app.status().staged.is_empty(),
        "a commit made outside the app empties the staged zone"
    );
    assert!(
        app.working().is_none(),
        "an empty status closes the working panel"
    );
}

#[test]
fn an_external_stage_shows_up_without_a_filesystem_event() {
    let dir = TempDir::new().unwrap();
    staged_repo(dir.path());
    git(dir.path(), &["commit", "-qm", "outside"]);
    let mut app = App::open(dir.path()).unwrap();

    assert!(
        app.status().staged.is_empty(),
        "the repository starts clean"
    );

    std::fs::write(dir.path().join("one.txt"), "one again\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    app.poll_status();

    let paths: Vec<&str> = app
        .status()
        .staged
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(
        paths,
        vec!["one.txt"],
        "staging from outside the app shows up on the next poll"
    );
}
