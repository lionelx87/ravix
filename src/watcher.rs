use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use git2::Repository;
use notify::event::ModifyKind;
use notify::{EventKind, RecursiveMode, Watcher};

const MAX_COALESCE: Duration = Duration::from_millis(500);

/// Watches a repository's `.git` directory and working tree, coalescing bursts
/// of filesystem events into a single debounced reload signal. Worktree events
/// on git-ignored paths (build artifacts, caches) are dropped so background
/// tooling never triggers reloads, while nested `.git` directories stay
/// relevant so submodule commits still reload. A burst never coalesces for
/// longer than [`MAX_COALESCE`], so a neighbouring process writing without
/// pause cannot hold the reload back forever. Watch registration can take
/// seconds on large worktrees, so the watcher is built entirely on a
/// background thread, and it signals once when registration completes to cover
/// whatever changed during that blind window.
pub struct RepoWatcher {
    reloads: Receiver<()>,
    degraded: Arc<AtomicBool>,
}

impl RepoWatcher {
    pub fn new(git_dir: &Path, workdir: Option<&Path>, debounce: Duration) -> Self {
        let (reload_tx, reload_rx) = mpsc::channel();
        let degraded = Arc::new(AtomicBool::new(false));
        let git_dir = git_dir.to_path_buf();
        let workdir = workdir.map(Path::to_path_buf);
        let flag = Arc::clone(&degraded);
        thread::spawn(move || watch_and_debounce(git_dir, workdir, debounce, reload_tx, flag));
        Self {
            reloads: reload_rx,
            degraded,
        }
    }

    pub fn changed(&self) -> bool {
        let mut changed = false;
        while self.reloads.try_recv().is_ok() {
            changed = true;
        }
        changed
    }

    pub fn degraded(&self) -> bool {
        self.degraded.load(Ordering::Relaxed)
    }
}

fn watch_and_debounce(
    git_dir: PathBuf,
    workdir: Option<PathBuf>,
    debounce: Duration,
    reload: mpsc::Sender<()>,
    degraded: Arc<AtomicBool>,
) {
    let filter = EventFilter::new(&git_dir, workdir.as_deref());
    let (raw_tx, raw_rx) = mpsc::channel();
    let registered_tx = raw_tx.clone();
    let event_flag = Arc::clone(&degraded);
    let Ok(mut watcher) =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(event) => {
                if filter.should_reload(&event) {
                    let _ = raw_tx.send(());
                }
            }
            Err(_) => {
                event_flag.store(true, Ordering::Relaxed);
                let _ = raw_tx.send(());
            }
        })
    else {
        degraded.store(true, Ordering::Relaxed);
        return;
    };
    if watcher.watch(&git_dir, RecursiveMode::Recursive).is_err() {
        degraded.store(true, Ordering::Relaxed);
        return;
    }
    if let Some(workdir) = &workdir
        && watcher.watch(workdir, RecursiveMode::Recursive).is_err()
    {
        degraded.store(true, Ordering::Relaxed);
    }
    if registered_tx.send(()).is_err() {
        return;
    }
    debounce_loop(raw_rx, reload, debounce, MAX_COALESCE);
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
        if holds_git_dir(relative) {
            return true;
        }
        match &self.repo {
            Some(repo) => !repo.is_path_ignored(relative).unwrap_or(false),
            None => true,
        }
    }
}

fn holds_git_dir(path: &Path) -> bool {
    path.components()
        .any(|component| component == Component::Normal(".git".as_ref()))
}

fn signals_change(kind: &EventKind) -> bool {
    !matches!(
        kind,
        EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(_))
    )
}

fn debounce_loop(
    raw: Receiver<()>,
    reload: mpsc::Sender<()>,
    debounce: Duration,
    max_coalesce: Duration,
) {
    while raw.recv().is_ok() {
        let burst_started = Instant::now();
        loop {
            let quiet = debounce.min(max_coalesce.saturating_sub(burst_started.elapsed()));
            if quiet.is_zero() {
                break;
            }
            match raw.recv_timeout(quiet) {
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
    use std::sync::atomic::AtomicBool;

    use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, MetadataKind, RemoveKind};

    use super::*;

    #[test]
    fn an_unbroken_stream_of_events_still_reloads() {
        let (raw_tx, raw_rx) = mpsc::channel();
        let (reload_tx, reload_rx) = mpsc::channel();
        let churning = Arc::new(AtomicBool::new(true));
        let stop = Arc::clone(&churning);
        let churn = thread::spawn(move || {
            while stop.load(Ordering::Relaxed) && raw_tx.send(()).is_ok() {
                thread::sleep(Duration::from_millis(2));
            }
        });
        let debounce = thread::spawn(move || {
            debounce_loop(
                raw_rx,
                reload_tx,
                Duration::from_millis(50),
                Duration::from_millis(200),
            );
        });

        let reloaded = reload_rx.recv_timeout(Duration::from_secs(2));
        churning.store(false, Ordering::Relaxed);
        churn.join().unwrap();
        drop(reload_rx);
        debounce.join().unwrap();

        assert!(
            reloaded.is_ok(),
            "a process writing without pause held the reload back"
        );
    }

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
    fn nested_git_directories_stay_relevant() {
        assert!(holds_git_dir(Path::new("sub/.git/index")));
        assert!(holds_git_dir(Path::new(".git/refs/heads/main")));
        assert!(!holds_git_dir(Path::new("src/.gitignore")));
        assert!(!holds_git_dir(Path::new("node_modules/pkg/index.js")));
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
