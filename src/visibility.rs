use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibilitySnapshot {
    pub hidden: Vec<String>,
    pub pinned: Vec<String>,
}

#[derive(Default)]
pub struct Visibility {
    hidden: HashSet<String>,
    pinned: HashSet<String>,
}

impl Visibility {
    pub fn toggle(&mut self, name: &str) {
        if !self.hidden.remove(name) {
            self.hidden.insert(name.to_string());
        }
    }

    pub fn pin(&mut self, name: &str) {
        if !self.pinned.remove(name) {
            self.pinned.insert(name.to_string());
        }
    }

    pub fn solo(&mut self, name: &str, all: &[&str]) {
        self.hidden = all
            .iter()
            .filter(|&&other| other != name && !self.pinned.contains(other))
            .map(|&other| other.to_string())
            .collect();
    }

    pub fn is_visible(&self, name: &str, is_head: bool) -> bool {
        is_head || self.pinned.contains(name) || !self.hidden.contains(name)
    }

    pub fn is_pinned(&self, name: &str) -> bool {
        self.pinned.contains(name)
    }

    pub fn snapshot(&self) -> VisibilitySnapshot {
        VisibilitySnapshot {
            hidden: sorted(&self.hidden),
            pinned: sorted(&self.pinned),
        }
    }

    pub fn restore(&mut self, snapshot: VisibilitySnapshot) {
        self.hidden = snapshot.hidden.into_iter().collect();
        self.pinned = snapshot.pinned.into_iter().collect();
    }
}

fn sorted(set: &HashSet<String>) -> Vec<String> {
    let mut names: Vec<String> = set.iter().cloned().collect();
    names.sort();
    names
}
