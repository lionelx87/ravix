use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use git2::Repository;
use notify::event::ModifyKind;
use notify::{EventKind, RecursiveMode, Watcher};

/// Watches a repository's `.git` directory and working tree, coalescing bursts
/// of filesystem events into a single debounced reload signal. Worktree events
/// on git-ignored paths (build artifacts, caches) are dropped so background
/// tooling never triggers reloads. Watch registration can take seconds on
/// large worktrees, so the watcher is built entirely on a background thread
/// and starts signaling once registration completes.
pub struct RepoWatcher {
    reloads: Receiver<()>,
}

impl RepoWatcher {
    pub fn new(git_dir: &Path, workdir: Option<&Path>, debounce: Duration) -> Self {
        let (reload_tx, reload_rx) = mpsc::channel();
        let git_dir = git_dir.to_path_buf();
        let workdir = workdir.map(Path::to_path_buf);
        thread::spawn(move || watch_and_debounce(git_dir, workdir, debounce, reload_tx));
        Self { reloads: reload_rx }
    }

    pub fn changed(&self) -> bool {
        let mut changed = false;
        while self.reloads.try_recv().is_ok() {
            changed = true;
        }
        changed
    }
}

fn watch_and_debounce(
    git_dir: PathBuf,
    workdir: Option<PathBuf>,
    debounce: Duration,
    reload: mpsc::Sender<()>,
) {
    let filter = EventFilter::new(&git_dir, workdir.as_deref());
    let (raw_tx, raw_rx) = mpsc::channel();
    let Ok(mut watcher) =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok_and(|event| filter.should_reload(&event)) {
                let _ = raw_tx.send(());
            }
        })
    else {
        return;
    };
    if watcher.watch(&git_dir, RecursiveMode::Recursive).is_err() {
        return;
    }
    if let Some(workdir) = &workdir {
        let _ = watcher.watch(workdir, RecursiveMode::Recursive);
    }
    debounce_loop(raw_rx, reload, debounce);
}

struct EventFilter {
    git_dir: PathBuf,
    worktree: Option<WorktreeFilter>,
}

struct WorktreeFilter {
    root: PathBuf,
    repo: Option<Repository>,
}

impl EventFilter {
    fn new(git_dir: &Path, workdir: Option<&Path>) -> Self {
        Self {
            git_dir: git_dir.to_path_buf(),
            worktree: workdir.map(|root| WorktreeFilter {
                root: root.to_path_buf(),
                repo: Repository::open(root).ok(),
            }),
        }
    }

    fn should_reload(&self, event: &notify::Event) -> bool {
        if !signals_change(&event.kind) {
            return false;
        }
        if event.paths.is_empty() {
            return true;
        }
        event.paths.iter().any(|path| self.path_is_relevant(path))
    }

    fn path_is_relevant(&self, path: &Path) -> bool {
        if path.starts_with(&self.git_dir) {
            return true;
        }
        match &self.worktree {
            Some(worktree) => worktree.is_relevant(path),
            None => true,
        }
    }
}

impl WorktreeFilter {
    fn is_relevant(&self, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return true;
        };
        if relative.as_os_str().is_empty() {
            return true;
        }
        match &self.repo {
            Some(repo) => !repo.is_path_ignored(relative).unwrap_or(false),
            None => true,
        }
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
