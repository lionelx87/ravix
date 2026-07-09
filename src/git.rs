use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use git2::{BranchType, Oid, Repository, Sort};

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
