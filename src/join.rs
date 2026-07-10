#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergePrediction {
    UpToDate,
    FastForward,
    Clean,
    Conflicts { files: Vec<String> },
}

#[derive(Debug, Clone, Copy)]
pub struct Ancestry {
    pub source_in_head: bool,
    pub head_in_source: bool,
}

#[derive(Debug, Clone)]
pub struct MergeTreeResult {
    pub conflicted: bool,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinStrategy {
    FastForward,
    MergeCommit,
    CherryPick,
    Rebase,
}

pub struct JoinOption {
    pub strategy: JoinStrategy,
    pub label: String,
    pub note: String,
    pub enabled: bool,
}

pub struct JoinMenu {
    pub title: String,
    pub source_name: Option<String>,
    pub source_oid: String,
    pub options: Vec<JoinOption>,
    pub conflict_files: Vec<String>,
    pub summary: String,
    pub selected: usize,
    pub slide: f32,
    pub target: f32,
}

impl JoinMenu {
    pub fn focused(&self) -> Option<&JoinOption> {
        self.options.get(self.selected)
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.options.is_empty() {
            self.selected = 0;
            return;
        }
        let last = (self.options.len() - 1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
    }
}

pub fn classify(ancestry: Ancestry, merge_tree: &MergeTreeResult) -> MergePrediction {
    if ancestry.source_in_head {
        MergePrediction::UpToDate
    } else if ancestry.head_in_source {
        MergePrediction::FastForward
    } else if merge_tree.conflicted {
        MergePrediction::Conflicts {
            files: merge_tree.files.clone(),
        }
    } else {
        MergePrediction::Clean
    }
}
