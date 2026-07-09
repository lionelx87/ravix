use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// Watches a repository's `.git` directory and coalesces bursts of filesystem
/// events into a single debounced reload signal.
pub struct RepoWatcher {
    _watcher: RecommendedWatcher,
    reloads: Receiver<()>,
}

impl RepoWatcher {
    pub fn new(git_dir: &Path, debounce: Duration) -> notify::Result<Self> {
        let (raw_tx, raw_rx) = mpsc::channel();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event.is_ok() {
                    let _ = raw_tx.send(());
                }
            })?;
        watcher.watch(git_dir, RecursiveMode::Recursive)?;

        let (reload_tx, reload_rx) = mpsc::channel();
        thread::spawn(move || debounce_loop(raw_rx, reload_tx, debounce));

        Ok(Self {
            _watcher: watcher,
            reloads: reload_rx,
        })
    }

    pub fn changed(&self) -> bool {
        let mut changed = false;
        while self.reloads.try_recv().is_ok() {
            changed = true;
        }
        changed
    }
}

fn debounce_loop(raw: Receiver<()>, reload: mpsc::Sender<()>, debounce: Duration) {
    while raw.recv().is_ok() {
        loop {
            match raw.recv_timeout(debounce) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if reload.send(()).is_err() {
            return;
        }
    }
}
