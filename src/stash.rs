use crate::git::{FileStatus, LineStats};
use crate::staging::FileDiff;

pub const LIST_FORMAT: &str = "%gd%x00%ct%x00%gs%x00%H";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StashMessage {
    Named(String),
    Auto { base_id: String, base_summary: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    pub index: usize,
    pub oid: String,
    pub branch: Option<String>,
    pub time: i64,
    pub message: StashMessage,
}

impl StashMessage {
    pub fn text(&self) -> &str {
        match self {
            Self::Named(text) => text,
            Self::Auto { .. } => "no message",
        }
    }

    pub fn base_id(&self) -> Option<&str> {
        match self {
            Self::Named(_) => None,
            Self::Auto { base_id, .. } => Some(base_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashConflict {
    pub index: usize,
    pub message: String,
    pub pop: bool,
}

pub fn parse_stash_list(text: &str) -> Vec<StashEntry> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<StashEntry> {
    let mut fields = line.split('\u{0}');
    let index = parse_index(fields.next()?)?;
    let time = fields.next()?.trim().parse().ok()?;
    let subject = fields.next()?;
    let oid = fields.next()?.trim().to_string();
    let (branch, message) = parse_subject(subject)?;
    Some(StashEntry {
        index,
        oid,
        branch,
        time,
        message,
    })
}

fn parse_index(selector: &str) -> Option<usize> {
    let rest = selector.trim().strip_prefix("stash@{")?;
    let (index, _) = rest.split_once('}')?;
    index.trim().parse().ok()
}

fn parse_subject(subject: &str) -> Option<(Option<String>, StashMessage)> {
    let (auto, rest) = match subject.strip_prefix("WIP on ") {
        Some(rest) => (true, rest),
        None => (false, subject.strip_prefix("On ")?),
    };
    let (branch, text) = rest.split_once(": ")?;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let branch = (branch != "(no branch)").then(|| branch.to_string());
    let message = if auto {
        let (base_id, base_summary) = text.split_once(' ').unwrap_or((text, ""));
        StashMessage::Auto {
            base_id: base_id.to_string(),
            base_summary: base_summary.to_string(),
        }
    } else {
        StashMessage::Named(text.to_string())
    };
    Some((branch, message))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StashStats {
    pub files: usize,
    pub added: usize,
    pub removed: usize,
}

#[derive(Debug, Clone)]
pub struct StashFile {
    pub path: String,
    pub status: FileStatus,
    pub stats: LineStats,
    pub untracked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Entries,
    Files,
    Hunks,
}

pub type StashPanel = crate::slide::SlidePanel<StashEntry>;

pub struct StashView {
    pub panel: StashPanel,
    pub focus: Focus,
    pub files: Vec<StashFile>,
    pub file: usize,
    pub diff: Option<FileDiff>,
    pub fullscreen: bool,
    pub split: bool,
    pub hunk: usize,
    pub diff_scroll: u16,
    pub diff_hscroll: u16,
    pub files_scroll: u16,
    pub hunk_snap: bool,
}

impl StashView {
    pub fn opening(entries: Vec<StashEntry>) -> Self {
        Self {
            panel: StashPanel::opening(entries),
            focus: Focus::Entries,
            files: Vec::new(),
            file: 0,
            diff: None,
            fullscreen: false,
            split: false,
            hunk: 0,
            diff_scroll: 0,
            diff_hscroll: 0,
            files_scroll: 0,
            hunk_snap: false,
        }
    }

    pub fn focused_entry(&self) -> Option<&StashEntry> {
        self.panel.focused()
    }

    pub fn focused_file(&self) -> Option<&StashFile> {
        self.files.get(self.file)
    }

    pub fn move_file(&mut self, delta: isize) {
        self.file = crate::slide::clamp_index(self.file, delta, self.files.len());
        self.diff_scroll = 0;
        self.diff_hscroll = 0;
        self.hunk = 0;
    }

    pub fn move_hunk(&mut self, delta: isize) {
        let len = self.diff.as_ref().map_or(0, |diff| diff.hunks.len());
        if len == 0 {
            self.hunk = 0;
            return;
        }
        let last = (len - 1) as isize;
        self.hunk = (self.hunk as isize + delta).clamp(0, last) as usize;
        self.hunk_snap = true;
    }

    pub fn rotate_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Entries if !self.files.is_empty() => Focus::Files,
            Focus::Files if self.diff.is_some() => Focus::Hunks,
            _ => Focus::Entries,
        };
    }
}
