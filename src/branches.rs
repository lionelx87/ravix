#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInput {
    pub name: String,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchEntry {
    pub name: String,
    pub is_head: bool,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
}

pub fn branch_list(branches: &[BranchInput], head: Option<&str>) -> Vec<BranchEntry> {
    let mut entries: Vec<BranchEntry> = branches
        .iter()
        .map(|branch| BranchEntry {
            name: branch.name.clone(),
            is_head: head == Some(branch.name.as_str()),
            upstream: branch.upstream.clone(),
            ahead: branch.ahead,
            behind: branch.behind,
        })
        .collect();

    entries.sort_by(|a, b| b.is_head.cmp(&a.is_head).then_with(|| a.name.cmp(&b.name)));
    entries
}

pub struct BranchPanel {
    pub entries: Vec<BranchEntry>,
    pub selected: usize,
    pub slide: f32,
    pub target: f32,
}

impl BranchPanel {
    pub fn opening(entries: Vec<BranchEntry>) -> Self {
        Self {
            entries,
            selected: 0,
            slide: 0.0,
            target: 1.0,
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            self.selected = 0;
            return;
        }
        let last = (self.entries.len() - 1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
    }

    pub fn focused(&self) -> Option<&BranchEntry> {
        self.entries.get(self.selected)
    }
}
