use std::time::{Duration, Instant};

use ravix::watcher::RepoWatcher;

#[test]
fn watcher_construction_does_not_block_startup() {
    let Ok(repo) = std::env::var("RAVIX_BENCH_REPO") else {
        return;
    };
    let repo = std::path::PathBuf::from(repo);
    let started = Instant::now();
    let watcher = RepoWatcher::new(&repo.join(".git"), Some(&repo), Duration::from_millis(200));
    let elapsed = started.elapsed();
    drop(watcher);
    assert!(
        elapsed < Duration::from_millis(100),
        "RepoWatcher::new blocked for {elapsed:?}"
    );
}
