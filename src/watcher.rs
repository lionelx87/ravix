use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use git2::Repository;
use notify::event::ModifyKind;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

const MAX_COALESCE: Duration = Duration::from_millis(500);

/// Watches a repository's `.git` directory and working tree, coalescing bursts
/// of filesystem events into a single debounced reload signal. Git-ignored
/// directories are never registered, so build output and dependency trees cost
/// neither an inotify watch nor an event, while nested `.git` directories are
/// covered whole so submodule and nested-repository commits still reload.
/// Directories that appear later are picked up as they are created. A burst
/// never coalesces for longer than [`MAX_COALESCE`], so a neighbouring process
/// writing without pause cannot hold the reload back forever. Walking a large
/// worktree still costs time, so the watcher is built entirely on a background
/// thread, and it signals once when registration completes to cover whatever
/// changed during that blind window.
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

enum Signal {
    Changed,
    Discovered(PathBuf),
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
    let Ok(watcher) =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(event) => {
                if !filter.should_reload(&event) {
                    return;
                }
                if opens_directories(&event.kind) {
                    for path in event.paths.iter().filter(|path| filter.covers_dir(path)) {
                        let _ = raw_tx.send(Signal::Discovered(path.clone()));
                    }
                }
                let _ = raw_tx.send(Signal::Changed);
            }
            Err(_) => {
                event_flag.store(true, Ordering::Relaxed);
                let _ = raw_tx.send(Signal::Changed);
            }
        })
    else {
        degraded.store(true, Ordering::Relaxed);
        return;
    };
    let mut registry = WatchRegistry {
        watcher,
        filter: EventFilter::new(&git_dir, workdir.as_deref()),
        degraded: Arc::clone(&degraded),
    };
    if !registry.cover_whole(&git_dir) {
        return;
    }
    if let Some(workdir) = &workdir {
        registry.cover_unignored(workdir);
    }
    if registered_tx.send(Signal::Changed).is_err() {
        return;
    }
    debounce_loop(raw_rx, reload, debounce, MAX_COALESCE, |path| {
        registry.cover_unignored(path);
    });
}

/// Registers one inotify watch per directory that git actually tracks, so a
/// dependency or build tree never eats into the per-user watch budget. Nested
/// `.git` directories are taken whole: they hold the refs and index of
/// submodules and nested repositories, and none of it is ignorable.
struct WatchRegistry {
    watcher: RecommendedWatcher,
    filter: EventFilter,
    degraded: Arc<AtomicBool>,
}

impl WatchRegistry {
    fn cover_whole(&mut self, root: &Path) -> bool {
        if self.watcher.watch(root, RecursiveMode::Recursive).is_err() {
            self.degraded.store(true, Ordering::Relaxed);
            return false;
        }
        true
    }

    fn cover_unignored(&mut self, root: &Path) {
        let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            if self
                .watcher
                .watch(&dir, RecursiveMode::NonRecursive)
                .is_err()
            {
                self.degraded.store(true, Ordering::Relaxed);
                return;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }
                let path = entry.path();
                if path.starts_with(&self.filter.git_dir) {
                    continue;
                }
                if is_git_dir(&path) {
                    self.cover_whole(&path);
                } else if self.filter.covers_dir(&path) {
                    pending.push(path);
                }
            }
        }
    }
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

    fn covers_dir(&self, path: &Path) -> bool {
        if path.starts_with(&self.git_dir) || !path.is_dir() {
            return false;
        }
        if is_git_dir(path) {
            return true;
        }
        match &self.worktree {
            Some(worktree) => worktree.covers_dir(path),
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

    fn covers_dir(&self, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return false;
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

fn holds_git_dir(path: &Path) -> bool {
    path.components()
        .any(|component| component == Component::Normal(".git".as_ref()))
}

fn is_git_dir(path: &Path) -> bool {
    path.file_name() == Some(".git".as_ref())
}

fn opens_directories(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(_))
    )
}

fn signals_change(kind: &EventKind) -> bool {
    !matches!(
        kind,
        EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(_))
    )
}

fn debounce_loop(
    raw: Receiver<Signal>,
    reload: mpsc::Sender<()>,
    debounce: Duration,
    max_coalesce: Duration,
    mut cover: impl FnMut(&Path),
) {
    while let Ok(first) = raw.recv() {
        let mut owed = absorb(first, &mut cover);
        let burst_started = Instant::now();
        loop {
            let quiet = debounce.min(max_coalesce.saturating_sub(burst_started.elapsed()));
            if quiet.is_zero() {
                break;
            }
            match raw.recv_timeout(quiet) {
                Ok(signal) => {
                    owed |= absorb(signal, &mut cover);
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if owed && reload.send(()).is_err() {
            return;
        }
    }
}

fn absorb(signal: Signal, cover: &mut impl FnMut(&Path)) -> bool {
    match signal {
        Signal::Changed => true,
        Signal::Discovered(path) => {
            cover(&path);
            false
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
            while stop.load(Ordering::Relaxed) && raw_tx.send(Signal::Changed).is_ok() {
                thread::sleep(Duration::from_millis(2));
            }
        });
        let debounce = thread::spawn(move || {
            debounce_loop(
                raw_rx,
                reload_tx,
                Duration::from_millis(50),
                Duration::from_millis(200),
                |_| {},
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
