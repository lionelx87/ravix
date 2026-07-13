#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInput {
    pub name: String,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub remote: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchEntry {
    pub name: String,
    pub is_head: bool,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub remote: Option<String>,
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
            remote: branch.remote.clone(),
        })
        .collect();

    entries.sort_by(|a, b| {
        b.is_head
            .cmp(&a.is_head)
            .then_with(|| a.remote.is_some().cmp(&b.remote.is_some()))
            .then_with(|| a.name.cmp(&b.name))
    });
    entries
}

pub type BranchPanel = crate::slide::SlidePanel<BranchEntry>;
