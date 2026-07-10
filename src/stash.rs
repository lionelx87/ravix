#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
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

pub struct StashPanel {
    pub entries: Vec<StashEntry>,
    pub selected: usize,
    pub slide: f32,
    pub target: f32,
}

impl StashPanel {
    pub fn opening(entries: Vec<StashEntry>) -> Self {
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

    pub fn focused(&self) -> Option<&StashEntry> {
        self.entries.get(self.selected)
    }
}
