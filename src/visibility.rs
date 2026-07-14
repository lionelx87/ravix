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
        let target: HashSet<String> = all
            .iter()
            .filter(|&&other| other != name && !self.pinned.contains(other))
            .map(|&other| other.to_string())
            .collect();
        if self.hidden == target {
            self.hidden.clear();
        } else {
            self.hidden = target;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [&str; 3] = ["main", "alpha", "beta"];

    #[test]
    fn solo_hides_every_other_branch() {
        let mut visibility = Visibility::default();
        visibility.solo("main", &ALL);
        assert!(visibility.is_visible("main", false));
        assert!(!visibility.is_visible("alpha", false));
        assert!(!visibility.is_visible("beta", false));
    }

    #[test]
    fn solo_again_on_the_same_branch_restores_all() {
        let mut visibility = Visibility::default();
        visibility.solo("main", &ALL);
        visibility.solo("main", &ALL);
        assert!(visibility.is_visible("alpha", false));
        assert!(visibility.is_visible("beta", false));
    }

    #[test]
    fn solo_on_another_branch_re_solos_instead_of_restoring() {
        let mut visibility = Visibility::default();
        visibility.solo("main", &ALL);
        visibility.solo("alpha", &ALL);
        assert!(visibility.is_visible("alpha", false));
        assert!(!visibility.is_visible("main", false));
    }

    #[test]
    fn a_manual_toggle_after_solo_keeps_solo_re_armable() {
        let mut visibility = Visibility::default();
        visibility.solo("main", &ALL);
        visibility.toggle("alpha");
        visibility.solo("main", &ALL);
        assert!(!visibility.is_visible("alpha", false));
        visibility.solo("main", &ALL);
        assert!(visibility.is_visible("alpha", false));
    }
}
