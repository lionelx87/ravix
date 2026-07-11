use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submodule {
    pub name: String,
    pub path: String,
    pub initialized: bool,
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
