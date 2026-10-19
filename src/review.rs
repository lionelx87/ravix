use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::PathBuf;
use std::sync::mpsc;

use git2::Oid;

use crate::git::{CommitInfo, LineStats, Repo};
use crate::palette::fuzzy_filter;
use crate::staging::{FileDiff, Hunk};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewRow<Id> {
    Commit(Id),
    Merge(Id),
}

pub fn own_commits<Id: Clone + Eq + Hash>(
    tip: Id,
    base: Id,
    parents: impl Fn(&Id) -> Vec<Id>,
) -> Vec<ReviewRow<Id>> {
    let mut branch_line: Vec<(Id, usize)> = Vec::new();
    let mut branch_seen: HashSet<Id> = HashSet::new();
    let mut base_seen: HashSet<Id> = HashSet::new();
    let mut branch_cursor = Some(tip);
    let mut base_cursor = Some(base);

    let fork = loop {
        if branch_cursor.is_none() && base_cursor.is_none() {
            break None;
        }
        if let Some(id) = branch_cursor.take() {
            if base_seen.contains(&id) {
                break Some(id);
            }
            let parents = parents(&id);
            branch_cursor = parents.first().cloned();
            branch_seen.insert(id.clone());
            branch_line.push((id, parents.len()));
        }
        if let Some(id) = base_cursor.take() {
            if branch_seen.contains(&id) {
                break Some(id);
            }
            base_cursor = parents(&id).first().cloned();
            base_seen.insert(id);
        }
    };

    branch_line
        .into_iter()
        .take_while(|(id, _)| Some(id) != fork.as_ref())
        .map(|(id, parent_count)| {
            if parent_count > 1 {
                ReviewRow::Merge(id)
            } else {
                ReviewRow::Commit(id)
            }
        })
        .collect()
}

const TRUNKS: [&str; 5] = ["main", "master", "develop", "development", "trunk"];

pub fn base_candidates(
    branch: &str,
    upstream: Option<&str>,
    remote_default: Option<&str>,
    exists: impl Fn(&str) -> bool,
) -> Vec<String> {
    let trunks = TRUNKS
        .iter()
        .flat_map(|trunk| [trunk.to_string(), format!("origin/{trunk}")])
        .filter(|name| exists(name));
    let mut candidates: Vec<String> = Vec::new();
    for name in remote_default.map(str::to_string).into_iter().chain(trunks) {
        if name != branch && Some(name.as_str()) != upstream && !candidates.contains(&name) {
            candidates.push(name);
        }
    }
    candidates
}

pub fn nearest_base<Id: Clone + Eq + Hash>(
    tip: Id,
    candidates: &[(String, Id)],
    parents: impl Fn(&Id) -> Vec<Id>,
) -> Option<String> {
    candidates
        .iter()
        .min_by_key(|(_, base)| own_commits(tip.clone(), base.clone(), &parents).len())
        .map(|(name, _)| name.clone())
}

#[derive(Debug, Clone)]
pub struct CommitStats<Id> {
    pub id: Id,
    pub author: String,
    pub files: HashMap<String, LineStats>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTotal {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    pub commits: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewSummary {
    pub commits: usize,
    pub authors: Vec<String>,
    pub files: Vec<FileTotal>,
    pub added: usize,
    pub removed: usize,
}

pub fn summarize<Id>(stats: &[CommitStats<Id>]) -> ReviewSummary {
    let mut authors: Vec<String> = Vec::new();
    let mut files: HashMap<&str, FileTotal> = HashMap::new();
    for commit in stats {
        if !authors.contains(&commit.author) {
            authors.push(commit.author.clone());
        }
        for (path, lines) in &commit.files {
            let total = files.entry(path).or_insert_with(|| FileTotal {
                path: path.clone(),
                added: 0,
                removed: 0,
                commits: 0,
            });
            total.added += lines.added;
            total.removed += lines.removed;
            total.commits += 1;
        }
    }
    let mut files: Vec<FileTotal> = files.into_values().collect();
    files.sort_by(|a, b| {
        (b.added + b.removed)
            .cmp(&(a.added + a.removed))
            .then_with(|| a.path.cmp(&b.path))
    });
    ReviewSummary {
        commits: stats.len(),
        authors,
        added: files.iter().map(|file| file.added).sum(),
        removed: files.iter().map(|file| file.removed).sum(),
        files,
    }
}

pub fn commits_touching<Id: Clone>(stats: &[CommitStats<Id>], path: &str) -> Vec<Id> {
    stats
        .iter()
        .filter(|commit| commit.files.contains_key(path))
        .map(|commit| commit.id.clone())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewFocus {
    Commits,
    Files,
    Diff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    Kept,
    ChosenBaseGone,
    BranchGone,
    NoBase,
}

pub struct BranchReview {
    pub branch: String,
    pub base: String,
    pub base_chosen: bool,
    pub rows: Vec<ReviewRow<CommitInfo>>,
    pub selected: Option<Oid>,
    pub focus: ReviewFocus,
    pub file: usize,
    pub file_filter: Option<String>,
    pub query: Option<String>,
    pub query_editing: bool,
    pub diff: Option<FileDiff>,
    pub message: String,
    diff_key: Option<(Oid, Option<String>)>,
    pub split: bool,
    pub diff_scroll: u16,
    pub diff_hscroll: u16,
    pub stats: Vec<CommitStats<Oid>>,
    pub summary: Option<ReviewSummary>,
    pub unfold: f32,
    pub spinner: usize,
    unfold_target: f32,
    summary_rx: Option<mpsc::Receiver<Vec<CommitStats<Oid>>>>,
    diff_rx: Option<mpsc::Receiver<FileDiff>>,
}

impl BranchReview {
    pub fn open(repo: &Repo, branch: &str, base: &str) -> Option<Self> {
        let mut review = Self {
            branch: branch.to_string(),
            base: base.to_string(),
            base_chosen: false,
            rows: Vec::new(),
            selected: None,
            focus: ReviewFocus::Commits,
            file: 0,
            file_filter: None,
            query: None,
            query_editing: false,
            diff: None,
            message: String::new(),
            diff_key: None,
            split: false,
            diff_scroll: 0,
            diff_hscroll: 0,
            stats: Vec::new(),
            summary: None,
            unfold: 0.0,
            spinner: 0,
            unfold_target: 1.0,
            summary_rx: None,
            diff_rx: None,
        };
        review.reload(repo)?;
        Some(review)
    }

    pub fn reload(&mut self, repo: &Repo) -> Option<()> {
        let tip = repo.branch_oid(&self.branch)?;
        let base = repo.branch_oid(&self.base)?;
        let ids = own_commits(tip, base, |id| repo.parent_ids(*id));
        let oids: Vec<Oid> = ids.iter().map(|row| *row.id()).collect();
        let unchanged = self.rows.len() == oids.len()
            && self
                .rows
                .iter()
                .zip(&oids)
                .all(|(row, id)| row.id().id == *id);
        if unchanged {
            return Some(());
        }
        let previous_position = self.selected_position();
        let mut commits = repo.hydrate(&oids).into_iter();
        self.rows = ids
            .into_iter()
            .filter_map(|row| {
                let commit = commits.next()?;
                Some(match row {
                    ReviewRow::Commit(_) => ReviewRow::Commit(commit),
                    ReviewRow::Merge(_) => ReviewRow::Merge(commit),
                })
            })
            .collect();
        self.summary = None;
        self.stats = Vec::new();
        self.file = 0;
        self.file_filter = None;
        if self.focus == ReviewFocus::Files {
            self.focus = ReviewFocus::Commits;
        }
        self.reselect_near(repo, previous_position);
        self.request_summary(repo.git_dir().to_path_buf());
        Some(())
    }

    pub fn refresh(&mut self, repo: &Repo) -> Refresh {
        if repo.branch_oid(&self.branch).is_none() {
            return Refresh::BranchGone;
        }
        if repo.branch_oid(&self.base).is_some() {
            self.reload(repo);
            return Refresh::Kept;
        }
        let Some(base) = detect_base(repo, &self.branch) else {
            return Refresh::NoBase;
        };
        let was_chosen = std::mem::replace(&mut self.base_chosen, false);
        self.base = base;
        self.reload(repo);
        if was_chosen {
            Refresh::ChosenBaseGone
        } else {
            Refresh::Kept
        }
    }

    pub fn chosen(mut self) -> Self {
        self.base_chosen = true;
        self
    }

    fn request_summary(&mut self, git_dir: PathBuf) {
        let commits: Vec<(Oid, String)> = self
            .commits()
            .map(|commit| (commit.id, commit.author_name.clone()))
            .collect();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let Ok(repo) = Repo::discover(git_dir) else {
                return;
            };
            let stats = commits
                .into_iter()
                .map(|(id, author)| CommitStats {
                    id,
                    author,
                    files: repo.change_stats(id),
                })
                .collect();
            let _ = tx.send(stats);
        });
        self.summary_rx = Some(rx);
    }

    pub fn poll_background(&mut self) {
        if self.summary_pending() || self.diff_pending() {
            self.spinner = self.spinner.wrapping_add(1);
        }
        self.poll_diff();
        self.poll_summary();
    }

    fn poll_diff(&mut self) {
        let Some(rx) = &self.diff_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(diff) => {
                self.diff = Some(diff);
                self.diff_rx = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => self.diff_rx = None,
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    pub fn diff_pending(&self) -> bool {
        self.diff_rx.is_some()
    }

    fn poll_summary(&mut self) {
        let Some(rx) = &self.summary_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(stats) => {
                self.summary = Some(summarize(&stats));
                self.stats = stats;
                self.summary_rx = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => self.summary_rx = None,
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    pub fn change_base(&mut self, repo: &Repo, base: &str) {
        let previous = std::mem::replace(&mut self.base, base.to_string());
        if self.reload(repo).is_some() {
            self.base_chosen = true;
        } else {
            self.base = previous;
            self.reload(repo);
        }
    }

    pub fn fold(&mut self) {
        self.unfold_target = 0.0;
    }

    pub fn is_folding(&self) -> bool {
        self.unfold != self.unfold_target
    }

    pub fn advance_unfold(&mut self, step: f32) -> bool {
        crate::slide::advance(&mut self.unfold, self.unfold_target, step);
        self.unfold_target == 0.0 && self.unfold <= 0.0
    }

    pub fn summary_pending(&self) -> bool {
        self.summary_rx.is_some()
    }

    pub fn files(&self) -> &[FileTotal] {
        self.summary
            .as_ref()
            .map_or(&[], |summary| summary.files.as_slice())
    }

    pub fn visible_rows(&self) -> Vec<&ReviewRow<CommitInfo>> {
        let query = self.query.as_deref().filter(|query| !query.is_empty());
        if self.file_filter.is_none() && query.is_none() {
            return self.rows.iter().collect();
        }
        let touching = self
            .file_filter
            .as_deref()
            .map(|path| commits_touching(&self.stats, path));
        let commits: Vec<&CommitInfo> = self
            .commits()
            .filter(|commit| touching.as_ref().is_none_or(|ids| ids.contains(&commit.id)))
            .collect();
        let matching: Vec<usize> = match query {
            Some(query) => {
                let haystack: Vec<String> = commits
                    .iter()
                    .map(|commit| format!("{} {}", commit.summary, commit.author_name))
                    .collect();
                let refs: Vec<&str> = haystack.iter().map(String::as_str).collect();
                fuzzy_filter(query, &refs)
            }
            None => (0..commits.len()).collect(),
        };
        let kept: HashSet<Oid> = matching
            .into_iter()
            .map(|index| commits[index].id)
            .collect();
        self.rows
            .iter()
            .filter(|row| matches!(row, ReviewRow::Commit(commit) if kept.contains(&commit.id)))
            .collect()
    }

    pub fn start_query(&mut self) {
        self.query = Some(String::new());
        self.query_editing = true;
    }

    pub fn query_input(&mut self, repo: &Repo, character: char) {
        if let Some(query) = &mut self.query {
            query.push(character);
        }
        self.reselect(repo);
    }

    pub fn query_backspace(&mut self, repo: &Repo) {
        if let Some(query) = &mut self.query {
            query.pop();
        }
        self.reselect(repo);
    }

    pub fn submit_query(&mut self) {
        if self.query.as_deref().is_some_and(str::is_empty) {
            self.query = None;
        }
        self.query_editing = false;
    }

    pub fn clear_query(&mut self, repo: &Repo) {
        self.query = None;
        self.query_editing = false;
        self.reselect(repo);
    }

    pub fn is_filtered(&self) -> bool {
        self.file_filter.is_some() || self.query.is_some()
    }

    pub fn visible_commit_count(&self) -> usize {
        self.visible_commit_ids().len()
    }

    fn visible_commit_ids(&self) -> Vec<Oid> {
        self.visible_rows()
            .into_iter()
            .filter_map(ReviewRow::commit)
            .map(|commit| commit.id)
            .collect()
    }

    pub fn move_down(&mut self, repo: &Repo, delta: isize) {
        match self.focus {
            ReviewFocus::Commits => self.move_selection(repo, delta),
            ReviewFocus::Files => {
                self.file = crate::slide::clamp_index(self.file, delta, self.files().len());
            }
            ReviewFocus::Diff => {
                self.diff_scroll = (self.diff_scroll as isize + delta).max(0) as u16;
            }
        }
    }

    fn move_selection(&mut self, repo: &Repo, delta: isize) {
        let ids = self.visible_commit_ids();
        let Some(current) = ids.iter().position(|id| Some(*id) == self.selected) else {
            return;
        };
        let next = crate::slide::clamp_index(current, delta, ids.len());
        if next != current {
            self.selected = Some(ids[next]);
            self.sync_diff(repo);
        }
    }

    pub fn rotate_focus(&mut self) {
        self.focus = match self.focus {
            ReviewFocus::Commits if !self.files().is_empty() => ReviewFocus::Files,
            ReviewFocus::Commits | ReviewFocus::Files => ReviewFocus::Diff,
            ReviewFocus::Diff => ReviewFocus::Commits,
        };
    }

    pub fn rotate_focus_back(&mut self) {
        self.focus = match self.focus {
            ReviewFocus::Diff if !self.files().is_empty() => ReviewFocus::Files,
            ReviewFocus::Diff | ReviewFocus::Files => ReviewFocus::Commits,
            ReviewFocus::Commits => ReviewFocus::Diff,
        };
    }

    pub fn clear_filters(&mut self, repo: &Repo) -> bool {
        if !self.is_filtered() {
            return false;
        }
        self.query = None;
        self.file_filter = None;
        self.focus = ReviewFocus::Commits;
        self.reselect(repo);
        true
    }

    pub fn toggle_file_filter(&mut self, repo: &Repo) {
        if self.focus != ReviewFocus::Files {
            return;
        }
        let focused = self.files().get(self.file).map(|file| file.path.clone());
        self.file_filter = if self.file_filter == focused {
            None
        } else {
            focused
        };
        self.reselect(repo);
    }

    fn reselect(&mut self, repo: &Repo) {
        self.reselect_near(repo, 0);
    }

    fn reselect_near(&mut self, repo: &Repo, position: usize) {
        let ids = self.visible_commit_ids();
        if !self.selected.is_some_and(|id| ids.contains(&id)) {
            self.selected = ids.get(position.min(ids.len().saturating_sub(1))).copied();
        }
        self.sync_diff(repo);
    }

    fn selected_position(&self) -> usize {
        self.visible_commit_ids()
            .iter()
            .position(|id| Some(*id) == self.selected)
            .unwrap_or(0)
    }

    pub fn selected_commit(&self) -> Option<&CommitInfo> {
        self.commits()
            .find(|commit| Some(commit.id) == self.selected)
    }

    fn sync_diff(&mut self, repo: &Repo) {
        let key = self.selected.map(|id| (id, self.file_filter.clone()));
        if key == self.diff_key {
            return;
        }
        self.diff = None;
        self.diff_rx = key.clone().map(|(id, only)| {
            let git_dir = repo.git_dir().to_path_buf();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                if let Ok(repo) = Repo::discover(git_dir) {
                    let _ = tx.send(commit_diff(&repo, id, only.as_deref()));
                }
            });
            rx
        });
        self.message = self
            .selected
            .map(|id| repo.commit_message(id).trim_end().to_string())
            .unwrap_or_default();
        self.diff_key = key;
        self.diff_scroll = 0;
        self.diff_hscroll = 0;
    }

    pub fn commits(&self) -> impl Iterator<Item = &CommitInfo> {
        self.rows.iter().filter_map(ReviewRow::commit)
    }
}

impl<Id> ReviewRow<Id> {
    pub fn commit(&self) -> Option<&Id> {
        match self {
            ReviewRow::Commit(id) => Some(id),
            ReviewRow::Merge(_) => None,
        }
    }

    pub fn id(&self) -> &Id {
        match self {
            ReviewRow::Commit(id) | ReviewRow::Merge(id) => id,
        }
    }
}

pub fn detect_base(repo: &Repo, branch: &str) -> Option<String> {
    let tip = repo.branch_oid(branch)?;
    let candidates: Vec<(String, Oid)> = base_candidates(
        branch,
        repo.branch_upstream(branch).as_deref(),
        repo.remote_default().as_deref(),
        |name| repo.branch_exists(name),
    )
    .into_iter()
    .filter_map(|name| repo.branch_oid(&name).map(|oid| (name, oid)))
    .collect();
    nearest_base(tip, &candidates, |id| repo.parent_ids(*id))
}

fn commit_diff(repo: &Repo, id: Oid, only: Option<&str>) -> FileDiff {
    let files = repo.commit_diffs(id, only);
    let file_count = files.len();
    let hunks = files.into_iter().flat_map(|diff| {
        let path = diff.new_path;
        diff.hunks.into_iter().map(move |hunk| Hunk {
            header: format!("{path}  {}", hunk.header),
            lines: hunk.lines,
        })
    });
    let hunks = capped(hunks, DIFF_LINE_CAP);
    let plural = if file_count == 1 { "" } else { "s" };
    FileDiff {
        old_path: String::new(),
        new_path: format!("{file_count} file{plural}"),
        hunks,
    }
}

const DIFF_LINE_CAP: usize = 4_000;

fn capped(hunks: impl Iterator<Item = Hunk>, cap: usize) -> Vec<Hunk> {
    let mut kept = Vec::new();
    let mut shown = 0;
    let mut hidden = 0;
    for mut hunk in hunks {
        let room = cap.saturating_sub(shown);
        if hunk.lines.len() > room {
            hidden += hunk.lines.len() - room;
            hunk.lines.truncate(room);
        }
        shown += hunk.lines.len();
        if !hunk.lines.is_empty() {
            kept.push(hunk);
        }
    }
    if hidden > 0 {
        kept.push(Hunk {
            header: format!("… {hidden} more lines not shown"),
            lines: Vec::new(),
        });
    }
    kept
}

pub struct BasePicker {
    pub branch: String,
    pub query: String,
    names: Vec<String>,
    matches: Vec<usize>,
    pub selected: usize,
}

impl BasePicker {
    pub fn opening(branch: &str, names: Vec<String>) -> Self {
        let names: Vec<String> = names.into_iter().filter(|name| name != branch).collect();
        Self {
            branch: branch.to_string(),
            query: String::new(),
            matches: (0..names.len()).collect(),
            names,
            selected: 0,
        }
    }

    pub fn input(&mut self, character: char) {
        self.query.push(character);
        self.refilter();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.refilter();
    }

    fn refilter(&mut self) {
        let names: Vec<&str> = self.names.iter().map(String::as_str).collect();
        self.matches = fuzzy_filter(&self.query, &names);
        self.selected = 0;
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = crate::slide::clamp_index(self.selected, delta, self.matches.len());
    }

    pub fn rows(&self) -> Vec<&str> {
        self.matches
            .iter()
            .map(|&index| self.names[index].as_str())
            .collect()
    }

    pub fn chosen(&self) -> Option<&str> {
        let index = *self.matches.get(self.selected)?;
        Some(self.names[index].as_str())
    }
}
