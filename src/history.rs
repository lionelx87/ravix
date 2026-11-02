use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use git2::Oid;

use crate::git::{FileVersion, Repo};
use crate::palette::fuzzy_filter;
use crate::picker::BranchPicker;
use crate::staging::{DIFF_LINE_CAP, FileDiff, Hunk, capped};

const RECORD_START: char = '\u{1e}';
const FIELD_SEP: char = '\u{1f}';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow<Id> {
    pub id: Option<Id>,
    pub parents: Vec<Id>,
    pub author: String,
    pub time: i64,
    pub subject: String,
    pub path: String,
    pub renamed_from: Option<String>,
    pub added: usize,
    pub removed: usize,
    pub binary: bool,
}

pub fn parse_log(text: &str) -> Vec<HistoryRow<String>> {
    text.split(RECORD_START).filter_map(parse_record).collect()
}

pub fn parse_record(record: &str) -> Option<HistoryRow<String>> {
    let (header, stats) = record.split_once('\0')?;
    let mut fields = header.split(FIELD_SEP);
    let id = fields.next()?.to_string();
    let parents = fields
        .next()?
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let author = fields.next()?.to_string();
    let time = fields.next()?.parse().ok()?;
    let subject = fields.next().unwrap_or_default().to_string();

    let mut tokens = stats.trim_start_matches(['\n', '\0']).split('\0');
    let numstat = tokens.next()?;
    let mut columns = numstat.splitn(3, '\t');
    let added = columns.next()?;
    let removed = columns.next()?;
    let inline_path = columns.next()?;
    let (path, renamed_from) = if inline_path.is_empty() {
        let old = tokens.next()?.to_string();
        let new = tokens.next()?.to_string();
        (new, Some(old))
    } else {
        (inline_path.to_string(), None)
    };
    let binary = added == "-";
    Some(HistoryRow {
        id: Some(id),
        parents,
        author,
        time,
        subject,
        path,
        renamed_from,
        added: added.parse().unwrap_or(0),
        removed: removed.parse().unwrap_or(0),
        binary,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin<Id> {
    Head,
    Commit(Id),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tip<Id> {
    Head,
    Commit(Id),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start<Id> {
    pub tip: Tip<Id>,
    pub preselect: Option<Id>,
    pub uncommitted: bool,
}

pub fn start_of<Id: Clone>(
    origin: Origin<Id>,
    reachable_from_head: impl FnOnce(&Id) -> bool,
    dirty: bool,
) -> Start<Id> {
    let tip = match &origin {
        Origin::Commit(id) if !reachable_from_head(id) => Tip::Commit(id.clone()),
        _ => Tip::Head,
    };
    let preselect = match origin {
        Origin::Head => None,
        Origin::Commit(id) => Some(id),
    };
    Start {
        uncommitted: dirty && matches!(tip, Tip::Head),
        tip,
        preselect,
    }
}

pub fn change_bar(change: usize, largest: usize, width: usize) -> usize {
    if change == 0 || largest == 0 {
        return 0;
    }
    (change * width).div_ceil(largest).clamp(1, width)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rev<Id> {
    Head,
    Worktree,
    Commit(Id),
    FirstParent(Id),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version<Id> {
    pub rev: Rev<Id>,
    pub path: String,
}

pub fn compared<Id: Clone>(
    rows: &[HistoryRow<Id>],
    selected: usize,
    anchor: Option<usize>,
) -> Option<(Version<Id>, Version<Id>)> {
    let row = rows.get(selected)?;
    match anchor.filter(|&anchor| anchor != selected) {
        Some(anchor) => {
            let older = rows.get(anchor.max(selected))?;
            let newer = rows.get(anchor.min(selected))?;
            Some((version_at(older), version_at(newer)))
        }
        None => {
            let before = match &row.id {
                Some(id) => Rev::FirstParent(id.clone()),
                None => Rev::Head,
            };
            let previous_path = row.renamed_from.as_ref().unwrap_or(&row.path).clone();
            Some((
                Version {
                    rev: before,
                    path: previous_path,
                },
                version_at(row),
            ))
        }
    }
}

fn version_at<Id: Clone>(row: &HistoryRow<Id>) -> Version<Id> {
    Version {
        rev: row.id.clone().map_or(Rev::Worktree, Rev::Commit),
        path: row.path.clone(),
    }
}

pub type FileFinder = BranchPicker<()>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKey {
    Uncommitted,
    Commit(Oid),
}

impl HistoryRow<Oid> {
    pub fn key(&self) -> RowKey {
        self.id.map_or(RowKey::Uncommitted, RowKey::Commit)
    }

    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }

    pub fn version_name(&self) -> String {
        match self.id {
            Some(_) => self.short_id(),
            None => "working tree".to_string(),
        }
    }

    pub fn short_id(&self) -> String {
        self.id
            .map(|id| id.to_string()[..7].to_string())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFocus {
    Commits,
    Diff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiffKey {
    selected: RowKey,
    anchor: Option<RowKey>,
    full_file: bool,
}

pub struct FileHistory {
    pub path: String,
    pub tip: Tip<Oid>,
    pub from: Option<String>,
    pub rows: Vec<HistoryRow<Oid>>,
    pub selected: Option<RowKey>,
    pub anchor: Option<RowKey>,
    pub focus: HistoryFocus,
    pub query: Option<String>,
    pub query_editing: bool,
    pub split: bool,
    pub full_file: bool,
    pub diff: Option<FileDiff>,
    pub message: String,
    pub diff_scroll: u16,
    pub diff_hscroll: u16,
    pub commits_scroll: u16,
    pub hunk: Option<usize>,
    pub hunk_snap: bool,
    pub unfold: f32,
    pub spinner: usize,
    unfold_target: f32,
    preselect: Option<Oid>,
    fallback_position: usize,
    moved: bool,
    head: Option<String>,
    diff_key: Option<DiffKey>,
    diff_rx: Option<mpsc::Receiver<FileDiff>>,
    rows_rx: Option<mpsc::Receiver<HistoryRow<String>>>,
}

impl FileHistory {
    pub fn open(repo: &Repo, path: &str, origin: Origin<Oid>, from: Option<String>) -> Self {
        let start = start_of(
            origin,
            |id| repo.is_head_ancestor(*id),
            repo.path_is_dirty(path),
        );
        let mut history = Self {
            path: path.to_string(),
            tip: start.tip,
            from: from.filter(|_| matches!(start.tip, Tip::Commit(_))),
            rows: Vec::new(),
            selected: None,
            anchor: None,
            focus: HistoryFocus::Commits,
            query: None,
            query_editing: false,
            split: false,
            full_file: false,
            diff: None,
            message: String::new(),
            diff_scroll: 0,
            diff_hscroll: 0,
            commits_scroll: 0,
            hunk: None,
            hunk_snap: false,
            unfold: 0.0,
            spinner: 0,
            unfold_target: 1.0,
            preselect: start.preselect,
            fallback_position: 0,
            moved: false,
            head: repo.head_oid(),
            diff_key: None,
            diff_rx: None,
            rows_rx: None,
        };
        if start.uncommitted {
            history.rows.push(uncommitted_row(repo, path));
            history.selected = Some(RowKey::Uncommitted);
        }
        history.load(repo);
        history.sync_diff(repo);
        history
    }

    fn load(&mut self, repo: &Repo) {
        let Some(workdir) = repo.workdir().map(Path::to_path_buf) else {
            return;
        };
        let tip = match self.tip {
            Tip::Head => "HEAD".to_string(),
            Tip::Commit(id) => id.to_string(),
        };
        self.rows_rx = Some(spawn_log(workdir, tip, self.path.clone()));
    }

    pub fn refresh(&mut self, repo: &Repo) {
        let head = repo.head_oid();
        let head_moved = head != self.head;
        self.head = head;
        if matches!(self.tip, Tip::Commit(_)) {
            return;
        }
        let dirty = repo.path_is_dirty(&self.path);
        let had_uncommitted = self.rows.first().is_some_and(|row| row.id.is_none());
        if head_moved {
            if let Some(RowKey::Commit(id)) = self.selected {
                self.preselect = Some(id);
                self.fallback_position = self.selected_position();
                self.moved = false;
            }
            self.rows.retain(|row| row.id.is_none());
            self.load(repo);
        }
        match (dirty, had_uncommitted) {
            (true, true) => self.rows[0] = uncommitted_row(repo, &self.path),
            (true, false) => self.rows.insert(0, uncommitted_row(repo, &self.path)),
            (false, true) => {
                self.rows.remove(0);
            }
            (false, false) => {}
        }
        if !dirty && self.anchor == Some(RowKey::Uncommitted) {
            self.anchor = None;
        }
        if self.selected == Some(RowKey::Uncommitted) || head_moved {
            self.diff_key = None;
        }
        self.reselect(repo);
    }

    pub fn loading(&self) -> bool {
        self.rows_rx.is_some()
    }

    pub fn diff_pending(&self) -> bool {
        self.diff_rx.is_some()
    }

    pub fn poll_background(&mut self, repo: &Repo) {
        if self.loading() || self.diff_pending() {
            self.spinner = self.spinner.wrapping_add(1);
        }
        self.poll_rows(repo);
        self.poll_diff();
    }

    fn poll_rows(&mut self, repo: &Repo) {
        let Some(rx) = &self.rows_rx else {
            return;
        };
        let mut arrived = false;
        loop {
            match rx.try_recv() {
                Ok(record) => {
                    if let Some(row) = typed_row(record) {
                        self.rows.push(row);
                        arrived = true;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.rows_rx = None;
                    break;
                }
            }
        }
        if arrived || !self.loading() {
            self.settle_selection(repo);
        }
    }

    fn settle_selection(&mut self, repo: &Repo) {
        let preselected = self
            .preselect
            .filter(|_| !self.moved)
            .map(RowKey::Commit)
            .filter(|key| self.rows.iter().any(|row| row.key() == *key));
        if let Some(key) = preselected {
            self.preselect = None;
            self.selected = Some(key);
            self.sync_diff(repo);
            return;
        }
        if self.loading() && self.selected.is_some() {
            self.sync_diff(repo);
            return;
        }
        if self
            .preselect
            .take()
            .is_some_and(|vanished| !self.moved && self.selected != Some(RowKey::Commit(vanished)))
        {
            self.selected = None;
        }
        self.reselect(repo);
        self.fallback_position = 0;
    }

    fn selected_position(&self) -> usize {
        self.visible_rows()
            .iter()
            .position(|row| Some(row.key()) == self.selected)
            .unwrap_or(0)
    }

    fn poll_diff(&mut self) {
        let Some(rx) = &self.diff_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(diff) => {
                if self.full_file
                    && let Some(first) = added_runs(&diff).first()
                {
                    self.diff_scroll = first.saturating_sub(FULL_FILE_LEAD) as u16;
                }
                self.diff = Some(diff);
                self.diff_rx = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => self.diff_rx = None,
            Err(mpsc::TryRecvError::Empty) => {}
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

    pub fn commit_count(&self) -> usize {
        self.rows.iter().filter(|row| row.id.is_some()).count()
    }

    pub fn largest_change(&self) -> usize {
        self.rows
            .iter()
            .map(|row| row.added + row.removed)
            .max()
            .unwrap_or(0)
    }

    pub fn visible_rows(&self) -> Vec<&HistoryRow<Oid>> {
        let Some(query) = self.query.as_deref().filter(|query| !query.is_empty()) else {
            return self.rows.iter().collect();
        };
        let commits: Vec<&HistoryRow<Oid>> =
            self.rows.iter().filter(|row| row.id.is_some()).collect();
        let haystack: Vec<String> = commits
            .iter()
            .map(|row| format!("{} {}", row.subject, row.author))
            .collect();
        let refs: Vec<&str> = haystack.iter().map(String::as_str).collect();
        let kept: HashSet<usize> = fuzzy_filter(query, &refs).into_iter().collect();
        commits
            .into_iter()
            .enumerate()
            .filter(|(index, _)| kept.contains(index))
            .map(|(_, row)| row)
            .collect()
    }

    pub fn is_filtered(&self) -> bool {
        self.query.is_some()
    }

    pub fn selected_row(&self) -> Option<&HistoryRow<Oid>> {
        self.rows
            .iter()
            .find(|row| Some(row.key()) == self.selected)
    }

    pub fn anchor_row(&self) -> Option<&HistoryRow<Oid>> {
        self.rows.iter().find(|row| Some(row.key()) == self.anchor)
    }

    fn index_of(&self, key: RowKey) -> Option<usize> {
        self.rows.iter().position(|row| row.key() == key)
    }

    pub fn in_anchor_range(&self, key: RowKey) -> bool {
        let (Some(anchor), Some(selected), Some(row)) = (
            self.anchor.and_then(|key| self.index_of(key)),
            self.selected.and_then(|key| self.index_of(key)),
            self.index_of(key),
        ) else {
            return false;
        };
        (anchor.min(selected)..=anchor.max(selected)).contains(&row)
    }

    pub fn move_down(&mut self, repo: &Repo, delta: isize) {
        match self.focus {
            HistoryFocus::Commits => self.move_selection(repo, delta),
            HistoryFocus::Diff => {
                self.diff_scroll = (self.diff_scroll as isize + delta).max(0) as u16;
                self.hunk_snap = false;
            }
        }
    }

    fn move_selection(&mut self, repo: &Repo, delta: isize) {
        let keys: Vec<RowKey> = self.visible_rows().iter().map(|row| row.key()).collect();
        let Some(current) = keys.iter().position(|key| Some(*key) == self.selected) else {
            return;
        };
        let next = crate::slide::clamp_index(current, delta, keys.len());
        if next != current {
            self.selected = Some(keys[next]);
            self.moved = true;
            self.sync_diff(repo);
        }
    }

    pub fn select_newest(&mut self, repo: &Repo) {
        let newest = self.visible_rows().first().map(|row| row.key());
        self.select_key(repo, newest);
    }

    pub fn select_oldest(&mut self, repo: &Repo) {
        let oldest = self.visible_rows().last().map(|row| row.key());
        self.select_key(repo, oldest);
    }

    fn select_key(&mut self, repo: &Repo, key: Option<RowKey>) {
        if key.is_some() {
            self.selected = key;
            self.moved = true;
            self.sync_diff(repo);
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            HistoryFocus::Commits => HistoryFocus::Diff,
            HistoryFocus::Diff => HistoryFocus::Commits,
        };
    }

    pub fn toggle_anchor(&mut self, repo: &Repo) {
        self.anchor = if self.anchor == self.selected {
            None
        } else {
            self.selected
        };
        self.sync_diff(repo);
    }

    pub fn toggle_full_file(&mut self, repo: &Repo) {
        self.full_file = !self.full_file;
        self.sync_diff(repo);
    }

    pub fn jump_hunk(&mut self, delta: isize) {
        let Some(diff) = &self.diff else {
            return;
        };
        if self.full_file {
            let runs = added_runs(diff);
            let current = self.diff_scroll as usize;
            let target = if delta > 0 {
                runs.iter().find(|&&run| run > current)
            } else {
                runs.iter().rev().find(|&&run| run < current)
            };
            if let Some(&target) = target {
                self.diff_scroll = target as u16;
            }
            return;
        }
        let count = diff
            .hunks
            .iter()
            .filter(|hunk| !hunk.lines.is_empty())
            .count();
        if count == 0 {
            return;
        }
        let next = match self.hunk {
            None if delta > 0 => 0,
            None => count - 1,
            Some(current) => crate::slide::clamp_index(current, delta, count),
        };
        self.hunk = Some(next);
        self.hunk_snap = true;
    }

    pub fn clear_layer(&mut self, repo: &Repo) -> bool {
        if self.query.is_some() {
            self.clear_query(repo);
            return true;
        }
        if self.anchor.is_some() {
            self.anchor = None;
            self.sync_diff(repo);
            return true;
        }
        false
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

    fn reselect(&mut self, repo: &Repo) {
        let keys: Vec<RowKey> = self.visible_rows().iter().map(|row| row.key()).collect();
        if !self.selected.is_some_and(|key| keys.contains(&key)) {
            let nearest = self.fallback_position.min(keys.len().saturating_sub(1));
            self.selected = keys.get(nearest).copied();
        }
        if !self.loading() && self.anchor.is_some_and(|key| self.index_of(key).is_none()) {
            self.anchor = None;
        }
        self.sync_diff(repo);
    }

    fn sync_diff(&mut self, repo: &Repo) {
        let loaded_anchor = self.anchor.filter(|key| self.index_of(*key).is_some());
        let key = self.selected.map(|selected| DiffKey {
            selected,
            anchor: loaded_anchor,
            full_file: self.full_file,
        });
        if key == self.diff_key {
            return;
        }
        self.diff_key = key;
        self.diff = None;
        self.diff_scroll = 0;
        self.diff_hscroll = 0;
        self.hunk = None;
        self.hunk_snap = false;
        let selected = self.selected.and_then(|key| self.index_of(key));
        let anchor = self.anchor.and_then(|key| self.index_of(key));
        self.message = self
            .selected_row()
            .and_then(|row| row.id)
            .map(|id| repo.commit_message(id).trim_end().to_string())
            .unwrap_or_else(|| "Uncommitted changes".to_string());
        let compared = selected.and_then(|selected| compared(&self.rows, selected, anchor));
        let Some((old, new)) = compared else {
            self.diff_rx = None;
            return;
        };
        let full_file = self.full_file;
        let git_dir = repo.git_dir().to_path_buf();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            if let Ok(repo) = Repo::discover(git_dir) {
                let _ = tx.send(version_diff(&repo, &old, &new, full_file));
            }
        });
        self.diff_rx = Some(rx);
    }
}

fn uncommitted_row(repo: &Repo, path: &str) -> HistoryRow<Oid> {
    let before = repo.file_version(&Rev::Head, path);
    let after = repo.file_version(&Rev::Worktree, path);
    let hunks = repo
        .blob_diff((bytes(&before), path), (bytes(&after), path), 0)
        .ok()
        .flatten()
        .unwrap_or_default();
    let (added, removed) = line_counts(&hunks);
    HistoryRow {
        id: None,
        parents: Vec::new(),
        author: String::new(),
        time: 0,
        subject: "Uncommitted changes".to_string(),
        path: path.to_string(),
        renamed_from: None,
        added,
        removed,
        binary: false,
    }
}

fn line_counts(hunks: &[Hunk]) -> (usize, usize) {
    hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .fold((0, 0), |(added, removed), line| match line.chars().next() {
            Some('+') => (added + 1, removed),
            Some('-') => (added, removed + 1),
            _ => (added, removed),
        })
}

fn bytes(version: &FileVersion) -> &[u8] {
    match version {
        FileVersion::Blob(bytes) => bytes,
        FileVersion::Absent | FileVersion::Submodule(_) => &[],
    }
}

fn typed_row(record: HistoryRow<String>) -> Option<HistoryRow<Oid>> {
    let id = Oid::from_str(record.id.as_deref()?).ok()?;
    Some(HistoryRow {
        id: Some(id),
        parents: record
            .parents
            .iter()
            .filter_map(|parent| Oid::from_str(parent).ok())
            .collect(),
        author: record.author,
        time: record.time,
        subject: record.subject,
        path: record.path,
        renamed_from: record.renamed_from,
        added: record.added,
        removed: record.removed,
        binary: record.binary,
    })
}

const FULL_FILE_LEAD: usize = 3;

pub const LOG_FORMAT: &str = "--format=%x1e%H%x1f%P%x1f%an%x1f%at%x1f%s";

fn spawn_log(workdir: PathBuf, tip: String, path: String) -> mpsc::Receiver<HistoryRow<String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok(mut child) = crate::git::follow_file_log(&workdir, &tip, &path) else {
            return;
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };
        let mut reader = BufReader::new(stdout);
        let mut chunk = Vec::new();
        loop {
            chunk.clear();
            match reader.read_until(RECORD_START as u8, &mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if chunk.last() == Some(&(RECORD_START as u8)) {
                chunk.pop();
            }
            let Some(record) = parse_record(&String::from_utf8_lossy(&chunk)) else {
                continue;
            };
            if record.parents.len() > 1 && !changed_by_merge(&workdir, &record) {
                continue;
            }
            if tx.send(record).is_err() {
                let _ = child.kill();
                break;
            }
        }
        let _ = child.wait();
    });
    rx
}

fn changed_by_merge(workdir: &Path, record: &HistoryRow<String>) -> bool {
    record
        .id
        .as_deref()
        .is_some_and(|id| crate::git::merge_changes_file(workdir, id, &record.path))
}

fn version_diff(repo: &Repo, old: &Version<Oid>, new: &Version<Oid>, full_file: bool) -> FileDiff {
    let before = repo.file_version(&old.rev, &old.path);
    let after = repo.file_version(&new.rev, &new.path);
    let hunks = match (&before, &after) {
        (_, FileVersion::Submodule(id)) => notice(format!("submodule → {}", &id.to_string()[..7])),
        (FileVersion::Submodule(_), _) => notice("submodule removed".to_string()),
        (_, FileVersion::Absent) if full_file => {
            notice("the file does not exist at this version".to_string())
        }
        _ => {
            let context = if full_file { 0 } else { 3 };
            match repo.blob_diff(
                (bytes(&before), &old.path),
                (bytes(&after), &new.path),
                context,
            ) {
                Ok(Some(hunks)) if full_file => full_file_hunks(bytes(&after), &hunks),
                Ok(Some(hunks)) => capped(hunks.into_iter(), DIFF_LINE_CAP),
                Ok(None) => notice("binary file changed".to_string()),
                Err(_) => Vec::new(),
            }
        }
    };
    FileDiff {
        old_path: old.path.clone(),
        new_path: new.path.clone(),
        hunks,
    }
}

fn notice(text: String) -> Vec<Hunk> {
    vec![Hunk {
        header: text,
        lines: Vec::new(),
    }]
}

fn full_file_hunks(content: &[u8], changes: &[Hunk]) -> Vec<Hunk> {
    let added = added_lines(changes);
    let text = String::from_utf8_lossy(content);
    let lines = text.lines().enumerate().map(|(index, line)| {
        let marker = if added.contains(&(index + 1)) {
            '+'
        } else {
            ' '
        };
        format!("{marker}{line}")
    });
    let shown: Vec<String> = lines.collect();
    let header = format!("full file · {} lines", shown.len());
    capped(
        std::iter::once(Hunk {
            header,
            lines: shown,
        }),
        DIFF_LINE_CAP,
    )
}

fn added_lines(hunks: &[Hunk]) -> HashSet<usize> {
    let mut added = HashSet::new();
    for hunk in hunks {
        let Some(mut line) = new_start(&hunk.header) else {
            continue;
        };
        for text in &hunk.lines {
            match text.chars().next() {
                Some('+') => {
                    added.insert(line);
                    line += 1;
                }
                Some('-') => {}
                _ => line += 1,
            }
        }
    }
    added
}

fn new_start(header: &str) -> Option<usize> {
    let after_plus = header.split_once('+')?.1;
    let digits: String = after_plus
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn added_runs(diff: &FileDiff) -> Vec<usize> {
    let Some(hunk) = diff.hunks.first() else {
        return Vec::new();
    };
    let mut runs = Vec::new();
    let mut previous_added = false;
    for (index, line) in hunk.lines.iter().enumerate() {
        let added = line.starts_with('+');
        if added && !previous_added {
            runs.push(index + 1);
        }
        previous_added = added;
    }
    runs
}
