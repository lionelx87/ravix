use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use git2::{BranchType, DiffOptions, Oid, Patch, Repository, Sort, StatusOptions};

use crate::staging::{FileDiff, Hunk};

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub id: Oid,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub time: i64,
    pub parents: Vec<Oid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Head,
    LocalBranch,
    Upstream,
}

#[derive(Debug, Clone)]
pub struct RefBadge {
    pub label: String,
    pub kind: BadgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
    Typechange,
    Other,
}

impl FileStatus {
    fn from_delta(delta: git2::Delta) -> Self {
        match delta {
            git2::Delta::Added => Self::Added,
            git2::Delta::Deleted => Self::Deleted,
            git2::Delta::Modified => Self::Modified,
            git2::Delta::Renamed => Self::Renamed,
            git2::Delta::Copied => Self::Copied,
            git2::Delta::Typechange => Self::Typechange,
            _ => Self::Other,
        }
    }

    pub fn code(self) -> char {
        match self {
            Self::Added => 'A',
            Self::Deleted => 'D',
            Self::Modified => 'M',
            Self::Renamed => 'R',
            Self::Copied => 'C',
            Self::Typechange => 'T',
            Self::Other => '?',
        }
    }
}

impl fmt::Display for FileStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub status: FileStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageState {
    Staged,
    Unstaged,
    Untracked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingFile {
    pub path: String,
    pub status: char,
    pub state: StageState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkingStatus {
    pub staged: Vec<WorkingFile>,
    pub unstaged: Vec<WorkingFile>,
    pub untracked: Vec<WorkingFile>,
}

impl WorkingStatus {
    pub fn is_empty(&self) -> bool {
        self.staged.is_empty() && self.unstaged.is_empty() && self.untracked.is_empty()
    }

    pub fn total(&self) -> usize {
        self.staged.len() + self.unstaged.len() + self.untracked.len()
    }
}

#[derive(Debug, Clone)]
pub struct RepoMeta {
    pub name: String,
    pub head_branch: Option<String>,
    pub badges: HashMap<Oid, Vec<RefBadge>>,
}

pub struct Repo {
    inner: Repository,
}

impl Repo {
    pub fn discover(path: impl AsRef<Path>) -> Result<Self, git2::Error> {
        Ok(Self {
            inner: Repository::discover(path)?,
        })
    }

    pub fn git_dir(&self) -> &Path {
        self.inner.path()
    }

    pub fn workdir(&self) -> Option<&Path> {
        self.inner.workdir()
    }

    pub fn name(&self) -> String {
        self.inner
            .workdir()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| "repository".to_string())
    }

    pub fn head_branch(&self) -> Option<String> {
        let head = self.inner.head().ok()?;
        if head.is_branch() {
            head.shorthand().ok().map(str::to_string)
        } else {
            None
        }
    }

    pub fn meta(&self) -> Result<RepoMeta, git2::Error> {
        Ok(RepoMeta {
            name: self.name(),
            head_branch: self.head_branch(),
            badges: self.collect_badges()?,
        })
    }

    pub fn commits(&self, skip: usize, limit: usize) -> Result<Vec<CommitInfo>, git2::Error> {
        let mut revwalk = self.inner.revwalk()?;
        revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
        for tip in self.visible_tips()? {
            revwalk.push(tip)?;
        }

        let mut commits = Vec::new();
        for oid in revwalk.skip(skip).take(limit) {
            let oid = oid?;
            let commit = self.inner.find_commit(oid)?;
            let author = commit.author();
            commits.push(CommitInfo {
                id: oid,
                short_id: shorten(oid),
                summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
                author_name: author.name().unwrap_or("").to_string(),
                author_email: author.email().unwrap_or("").to_string(),
                time: commit.time().seconds(),
                parents: commit.parent_ids().collect(),
            });
        }
        Ok(commits)
    }

    pub fn changed_files(&self, id: Oid) -> Result<Vec<FileChange>, git2::Error> {
        let commit = self.inner.find_commit(id)?;
        let tree = commit.tree()?;
        let parent_tree = match commit.parents().next() {
            Some(parent) => Some(parent.tree()?),
            None => None,
        };
        let diff = self
            .inner
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)?;

        let mut changes = Vec::new();
        for delta in diff.deltas() {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .and_then(Path::to_str)
                .unwrap_or("")
                .to_string();
            changes.push(FileChange {
                path,
                status: FileStatus::from_delta(delta.status()),
            });
        }
        Ok(changes)
    }

    pub fn working_status(&self) -> Result<WorkingStatus, git2::Error> {
        let mut options = StatusOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);
        let statuses = self.inner.statuses(Some(&mut options))?;

        let mut status = WorkingStatus::default();
        for entry in statuses.iter() {
            let flags = entry.status();
            let path = entry.path().unwrap_or("").to_string();

            if let Some(code) = index_status(flags) {
                status.staged.push(WorkingFile {
                    path: path.clone(),
                    status: code,
                    state: StageState::Staged,
                });
            }

            if flags.is_wt_new() {
                status.untracked.push(WorkingFile {
                    path,
                    status: '?',
                    state: StageState::Untracked,
                });
            } else if let Some(code) = worktree_status(flags) {
                status.unstaged.push(WorkingFile {
                    path,
                    status: code,
                    state: StageState::Unstaged,
                });
            }
        }
        Ok(status)
    }

    pub fn file_diff(&self, path: &str, staged: bool) -> Result<Option<FileDiff>, git2::Error> {
        let mut options = DiffOptions::new();
        options.pathspec(path).context_lines(3);

        let diff = if staged {
            let head_tree = self
                .inner
                .head()
                .ok()
                .and_then(|head| head.peel_to_tree().ok());
            self.inner
                .diff_tree_to_index(head_tree.as_ref(), None, Some(&mut options))?
        } else {
            self.inner.diff_index_to_workdir(None, Some(&mut options))?
        };

        for index in 0..diff.deltas().count() {
            let Some(patch) = Patch::from_diff(&diff, index)? else {
                continue;
            };
            let Some(delta) = diff.get_delta(index) else {
                continue;
            };
            let new_path = delta
                .new_file()
                .path()
                .and_then(Path::to_str)
                .unwrap_or(path);
            let old_path = delta
                .old_file()
                .path()
                .and_then(Path::to_str)
                .unwrap_or(new_path);
            if new_path != path && old_path != path {
                continue;
            }

            let mut hunks = Vec::new();
            for hunk_index in 0..patch.num_hunks() {
                let (hunk, line_count) = patch.hunk(hunk_index)?;
                let header = String::from_utf8_lossy(hunk.header())
                    .trim_end()
                    .to_string();
                let mut lines = Vec::new();
                for line_index in 0..line_count {
                    let line = patch.line_in_hunk(hunk_index, line_index)?;
                    let text = String::from_utf8_lossy(line.content());
                    let text = text.strip_suffix('\n').unwrap_or(&text);
                    let marker = match line.origin() {
                        '+' | '>' => '+',
                        '-' | '<' => '-',
                        _ => ' ',
                    };
                    lines.push(format!("{marker}{text}"));
                }
                hunks.push(Hunk { header, lines });
            }

            return Ok(Some(FileDiff {
                old_path: old_path.to_string(),
                new_path: new_path.to_string(),
                hunks,
            }));
        }

        Ok(None)
    }

    fn visible_tips(&self) -> Result<Vec<Oid>, git2::Error> {
        let mut tips = Vec::new();

        if let Ok(head) = self.inner.head()
            && let Some(oid) = head.target()
        {
            tips.push(oid);
        }

        for branch in self.inner.branches(Some(BranchType::Local))? {
            let (branch, _) = branch?;
            if let Some(oid) = branch.get().target() {
                tips.push(oid);
            }
            if let Ok(upstream) = branch.upstream()
                && let Some(oid) = upstream.get().target()
            {
                tips.push(oid);
            }
        }

        tips.sort();
        tips.dedup();
        Ok(tips)
    }

    fn collect_badges(&self) -> Result<HashMap<Oid, Vec<RefBadge>>, git2::Error> {
        let mut badges: HashMap<Oid, Vec<RefBadge>> = HashMap::new();

        if let Ok(head) = self.inner.head()
            && let Some(oid) = head.target()
        {
            badges.entry(oid).or_default().push(RefBadge {
                label: "HEAD".to_string(),
                kind: BadgeKind::Head,
            });
        }

        for branch in self.inner.branches(Some(BranchType::Local))? {
            let (branch, _) = branch?;
            let Some(oid) = branch.get().target() else {
                continue;
            };
            if let Some(name) = branch.name()?.map(str::to_string) {
                badges.entry(oid).or_default().push(RefBadge {
                    label: name,
                    kind: BadgeKind::LocalBranch,
                });
            }

            if let Ok(upstream) = branch.upstream()
                && let (Some(oid), Ok(Some(name))) = (upstream.get().target(), upstream.name())
            {
                badges.entry(oid).or_default().push(RefBadge {
                    label: name.to_string(),
                    kind: BadgeKind::Upstream,
                });
            }
        }

        Ok(badges)
    }
}

fn shorten(oid: Oid) -> String {
    oid.to_string().chars().take(7).collect()
}

fn index_status(flags: git2::Status) -> Option<char> {
    if flags.is_index_new() {
        Some('A')
    } else if flags.is_index_modified() {
        Some('M')
    } else if flags.is_index_deleted() {
        Some('D')
    } else if flags.is_index_renamed() {
        Some('R')
    } else if flags.is_index_typechange() {
        Some('T')
    } else {
        None
    }
}

fn worktree_status(flags: git2::Status) -> Option<char> {
    if flags.is_wt_modified() {
        Some('M')
    } else if flags.is_wt_deleted() {
        Some('D')
    } else if flags.is_wt_renamed() {
        Some('R')
    } else if flags.is_wt_typechange() {
        Some('T')
    } else {
        None
    }
}
