use crate::staging::FileDiff;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Files,
    Hunks,
}

pub struct WorkingView {
    pub fullscreen: bool,
    pub split: bool,
    pub slide: f32,
    pub target: f32,
    pub selected: usize,
    pub focus: Focus,
    pub diff: Option<FileDiff>,
    pub staged_side: bool,
    pub hunk: usize,
    pub diff_scroll: u16,
    pub diff_hscroll: u16,
    pub hunk_snap: bool,
}

impl WorkingView {
    pub fn opening() -> Self {
        Self {
            fullscreen: false,
            split: false,
            slide: 0.0,
            target: 1.0,
            selected: 0,
            focus: Focus::Files,
            diff: None,
            staged_side: false,
            hunk: 0,
            diff_scroll: 0,
            diff_hscroll: 0,
            hunk_snap: false,
        }
    }

    pub fn move_file(&mut self, delta: isize, len: usize) {
        self.selected = crate::slide::clamp_index(self.selected, delta, len);
        self.diff_scroll = 0;
        self.diff_hscroll = 0;
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

    pub fn reconcile(&mut self, len: usize) {
        self.selected = self.selected.min(len.saturating_sub(1));
        if self.focus == Focus::Hunks {
            let hunks = self.diff.as_ref().map_or(0, |diff| diff.hunks.len());
            if hunks == 0 {
                self.focus = Focus::Files;
                self.diff = None;
                self.hunk = 0;
            } else {
                self.hunk = self.hunk.min(hunks - 1);
            }
        }
    }
}
