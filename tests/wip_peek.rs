use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use ravix::app::{Action, App};

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

fn dirty_repo(dir: &TempDir) {
    let path = dir.path();
    let run = |args: &[&str]| git_run(path, args);
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "demo@ravix.dev"]);
    run(&["config", "user.name", "Demo"]);

    std::fs::write(path.join("app.txt"), "one\ntwo\nthree\n").unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "initial commit"]);

    std::fs::write(path.join("app.txt"), "ONE\ntwo\nthree\nfour\n").unwrap();
    std::fs::write(path.join("notes.txt"), "todo\n").unwrap();
}

#[test]
fn moving_onto_the_wip_row_shows_the_working_preview_not_the_stale_commit() {
    let dir = TempDir::new().unwrap();
    dirty_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    app.update(Action::OpenPanel);
    assert!(app.panel().is_some(), "the commit peek opens on HEAD");

    app.update(Action::SelectPrev);
    assert!(
        app.on_wip(),
        "selecting up from HEAD lands on the uncommitted-changes row"
    );
    assert!(
        app.panel().is_none(),
        "the stale commit panel is dismissed on the WIP row"
    );
    assert!(
        app.working().is_some(),
        "the WIP row shows the working-directory preview instead"
    );
}

#[test]
fn entering_the_wip_preview_does_not_leave_the_commit_panel_open_underneath() {
    let dir = TempDir::new().unwrap();
    dirty_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    app.update(Action::OpenPanel);
    app.update(Action::SelectPrev);
    app.update(Action::OpenPanel);

    assert!(
        app.working().is_some_and(|view| view.fullscreen),
        "Enter on the WIP row expands the working view to fullscreen"
    );
    assert!(
        app.panel().is_none(),
        "no commit panel remains rendered beside the WIP view"
    );
}

#[test]
fn the_peek_follows_back_to_a_commit_when_leaving_the_wip_row() {
    let dir = TempDir::new().unwrap();
    dirty_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    app.update(Action::OpenPanel);
    app.update(Action::SelectPrev);
    assert!(app.working().is_some());

    app.update(Action::SelectNext);
    assert!(!app.on_wip(), "moving down leaves the WIP row");
    assert!(
        app.working().is_none(),
        "the working preview closes when a commit is selected"
    );
    assert!(
        app.panel().is_some(),
        "the commit peek reopens for the selected commit"
    );
}
