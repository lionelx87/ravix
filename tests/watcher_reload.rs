mod common;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use ravix::watcher::RepoWatcher;

const DEBOUNCE: Duration = Duration::from_millis(30);
const SETTLE: Duration = Duration::from_millis(300);
const SIGNAL_TIMEOUT: Duration = Duration::from_secs(3);

fn committed_repo(dir: &TempDir) -> &Path {
    let path = dir.path();
    common::init_repo(path);
    std::fs::write(path.join("file.txt"), "v1").unwrap();
    common::git(path, &["add", "-A"]);
    common::git(path, &["commit", "-qm", "init"]);
    path
}

fn watcher_for(path: &Path) -> RepoWatcher {
    let watcher = RepoWatcher::new(&path.join(".git"), Some(path), DEBOUNCE);
    assert!(
        signals_within(&watcher, SIGNAL_TIMEOUT),
        "watcher did not signal once registration finished"
    );
    sleep(SETTLE);
    assert!(!watcher.changed(), "watcher signaled before any change");
    watcher
}

fn signals_within(watcher: &RepoWatcher, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if watcher.changed() {
            return true;
        }
        sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn git_dir_change_triggers_reload() {
    let dir = TempDir::new().unwrap();
    let path = committed_repo(&dir);
    let watcher = watcher_for(path);

    common::git(path, &["branch", "extra"]);

    assert!(
        signals_within(&watcher, SIGNAL_TIMEOUT),
        "ref update did not trigger a reload"
    );
}

#[test]
fn worktree_edit_triggers_reload() {
    let dir = TempDir::new().unwrap();
    let path = committed_repo(&dir);
    let watcher = watcher_for(path);

    std::fs::write(path.join("file.txt"), "v2").unwrap();

    assert!(
        signals_within(&watcher, SIGNAL_TIMEOUT),
        "modifying a tracked file did not trigger a reload"
    );
}

#[test]
fn ignored_path_does_not_trigger_reload() {
    let dir = TempDir::new().unwrap();
    let path = committed_repo(&dir);
    std::fs::write(path.join(".gitignore"), "build/\n").unwrap();
    common::git(path, &["add", ".gitignore"]);
    common::git(path, &["commit", "-qm", "chore: ignore build"]);
    std::fs::create_dir(path.join("build")).unwrap();
    let watcher = watcher_for(path);

    std::fs::write(path.join("build").join("artifact.o"), "obj").unwrap();

    assert!(
        !signals_within(&watcher, Duration::from_millis(600)),
        "a git-ignored file triggered a reload"
    );
}

#[test]
fn untracked_file_triggers_reload() {
    let dir = TempDir::new().unwrap();
    let path = committed_repo(&dir);
    let watcher = watcher_for(path);

    std::fs::write(path.join("new.txt"), "untracked").unwrap();

    assert!(
        signals_within(&watcher, SIGNAL_TIMEOUT),
        "creating an untracked file did not trigger a reload"
    );
}

#[test]
fn nested_git_dir_change_triggers_reload() {
    let dir = TempDir::new().unwrap();
    let path = committed_repo(&dir);
    let nested = path.join("sub");
    std::fs::create_dir(&nested).unwrap();
    common::init_repo(&nested);
    std::fs::write(nested.join("inner.txt"), "v1").unwrap();
    common::git(&nested, &["add", "-A"]);
    let watcher = watcher_for(path);

    common::git(&nested, &["commit", "-qm", "inner"]);

    assert!(
        signals_within(&watcher, SIGNAL_TIMEOUT),
        "a commit inside a nested repository did not trigger a reload"
    );
}

#[test]
fn a_neighbouring_process_writing_without_pause_still_reloads() {
    let dir = TempDir::new().unwrap();
    let path = committed_repo(&dir);
    let watcher = watcher_for(path);

    let churn_path = path.join("file.txt");
    let churning = Arc::new(AtomicBool::new(true));
    let stop = Arc::clone(&churning);
    let churn = std::thread::spawn(move || {
        let mut revision = 0u32;
        while stop.load(Ordering::Relaxed) {
            revision += 1;
            std::fs::write(&churn_path, format!("v{revision}")).unwrap();
            sleep(Duration::from_millis(5));
        }
    });

    let signaled = signals_within(&watcher, SIGNAL_TIMEOUT);
    churning.store(false, Ordering::Relaxed);
    churn.join().unwrap();

    assert!(
        signaled,
        "a neighbouring process writing without pause held the reload back"
    );
}
