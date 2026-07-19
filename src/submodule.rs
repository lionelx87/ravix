use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    Synced,
    Drifted,
    Uninitialized,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submodule {
    pub name: String,
    pub path: String,
    pub url: Option<String>,
    pub branch: Option<String>,
    pub checked_out: Option<String>,
    pub recorded: Option<String>,
    pub sync: SyncState,
    pub dirty: usize,
    pub untracked: usize,
    pub last_commit: Option<String>,
}

impl Submodule {
    pub fn initialized(&self) -> bool {
        self.sync != SyncState::Uninitialized
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty > 0 || self.untracked > 0
    }
}

pub fn breadcrumb_label(ancestors: &[PathBuf], current: &Path) -> String {
    ancestors
        .iter()
        .map(PathBuf::as_path)
        .chain(std::iter::once(current))
        .map(dir_name)
        .collect::<Vec<_>>()
        .join(" › ")
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
