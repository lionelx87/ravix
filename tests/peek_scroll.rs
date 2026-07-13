use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use ravix::app::{Action, App};
use ravix::working::Focus;

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

fn long_wip_repo(dir: &TempDir) {
    let path = dir.path();
    let run = |args: &[&str]| git_run(path, args);
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "demo@ravix.dev"]);
    run(&["config", "user.name", "Demo"]);

    let original: String = (0..60).map(|n| format!("line {n}\n")).collect();
    std::fs::write(path.join("app.txt"), &original).unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "seed"]);

    let edited: String = (0..60).map(|n| format!("CHANGED line {n}\n")).collect();
    std::fs::write(path.join("app.txt"), edited).unwrap();
}

fn open_wip_peek(app: &mut App) {
    app.update(Action::SelectPrev); // land on the WIP row
    app.update(Action::OpenPanel); // Enter opens the working preview
}

#[test]
fn focusing_the_peek_diff_lets_down_scroll_instead_of_changing_commit() {
    let dir = TempDir::new().unwrap();
    long_wip_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    open_wip_peek(&mut app);
    assert!(
        app.on_wip() && app.working().is_some(),
        "the WIP peek is open"
    );
    assert_eq!(app.working().unwrap().focus, Focus::Files);

    app.update(Action::ToggleFocus);
    assert_eq!(
        app.working().unwrap().focus,
        Focus::Hunks,
        "Tab focuses the peek diff"
    );

    app.update(Action::SelectNext);
    assert!(
        app.working().unwrap().diff_scroll > 0,
        "with the peek diff focused, Down scrolls it"
    );
    assert!(
        app.on_wip(),
        "scrolling the peek diff does not jump to the next commit"
    );
}

#[test]
fn unfocusing_the_peek_diff_restores_commit_navigation() {
    let dir = TempDir::new().unwrap();
    long_wip_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    open_wip_peek(&mut app);
    app.update(Action::ToggleFocus); // -> diff
    app.update(Action::SelectNext); // scroll
    app.update(Action::ToggleFocus); // -> back to files

    assert_eq!(app.working().unwrap().focus, Focus::Files);
    assert_eq!(
        app.working().unwrap().diff_scroll,
        0,
        "leaving the diff resets its scroll"
    );

    app.update(Action::SelectNext);
    assert!(
        !app.on_wip(),
        "back on the file/graph focus, Down leaves the WIP row for a commit"
    );
}
