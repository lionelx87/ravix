use crate::visibility::VisibilitySnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusSet {
    pub name: String,
    pub state: VisibilitySnapshot,
}

pub fn serialize(sets: &[FocusSet]) -> String {
    let mut out = String::new();
    for set in sets {
        out.push_str("focus ");
        out.push_str(&set.name);
        out.push('\n');
        for branch in &set.state.hidden {
            out.push_str("hide ");
            out.push_str(branch);
            out.push('\n');
        }
        for branch in &set.state.pinned {
            out.push_str("pin ");
            out.push_str(branch);
            out.push('\n');
        }
    }
    out
}

pub fn upsert(sets: &mut Vec<FocusSet>, set: FocusSet) {
    match sets.iter_mut().find(|existing| existing.name == set.name) {
        Some(existing) => *existing = set,
        None => sets.push(set),
    }
}

pub fn remove(sets: &mut Vec<FocusSet>, name: &str) {
    sets.retain(|set| set.name != name);
}

pub fn parse(text: &str) -> Vec<FocusSet> {
    let mut sets: Vec<FocusSet> = Vec::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("focus ") {
            sets.push(FocusSet {
                name: name.to_string(),
                state: VisibilitySnapshot {
                    hidden: Vec::new(),
                    pinned: Vec::new(),
                },
            });
        } else if let Some(branch) = line.strip_prefix("hide ")
            && let Some(set) = sets.last_mut()
        {
            set.state.hidden.push(branch.to_string());
        } else if let Some(branch) = line.strip_prefix("pin ")
            && let Some(set) = sets.last_mut()
        {
            set.state.pinned.push(branch.to_string());
        }
    }
    sets
}
