#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
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
    let rest = line.strip_prefix("stash@{")?;
    let (index, rest) = rest.split_once('}')?;
    let index = index.trim().parse().ok()?;
    let message = rest.strip_prefix(": ")?.trim().to_string();
    if message.is_empty() {
        return None;
    }
    Some(StashEntry { index, message })
}

pub type StashPanel = crate::slide::SlidePanel<StashEntry>;
