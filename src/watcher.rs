use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use notify::event::ModifyKind;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

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
                if event.is_ok_and(|event| signals_change(&event.kind)) {
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

fn signals_change(kind: &EventKind) -> bool {
    !matches!(
        kind,
        EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(_))
    )
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

#[cfg(test)]
mod tests {
    use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, MetadataKind, RemoveKind};

    use super::*;

    #[test]
    fn read_driven_events_are_ignored() {
        assert!(!signals_change(&EventKind::Access(AccessKind::Open(
            AccessMode::Read
        ))));
        assert!(!signals_change(&EventKind::Access(AccessKind::Close(
            AccessMode::Write
        ))));
        assert!(!signals_change(&EventKind::Modify(ModifyKind::Metadata(
            MetadataKind::AccessTime
        ))));
    }

    #[test]
    fn real_changes_still_signal() {
        assert!(signals_change(&EventKind::Create(CreateKind::File)));
        assert!(signals_change(&EventKind::Remove(RemoveKind::File)));
        assert!(signals_change(&EventKind::Modify(ModifyKind::Data(
            DataChange::Content
        ))));
        assert!(signals_change(&EventKind::Modify(ModifyKind::Name(
            notify::event::RenameMode::Any
        ))));
        assert!(signals_change(&EventKind::Any));
    }
}
