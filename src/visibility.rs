use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibilitySnapshot {
    pub hidden: Vec<String>,
}

#[derive(Default)]
pub struct Visibility {
    hidden: HashSet<String>,
    pinned: Vec<String>,
}

impl Visibility {
    pub fn toggle(&mut self, name: &str) {
        if !self.hidden.remove(name) {
            self.hidden.insert(name.to_string());
        }
    }

    pub fn toggle_pin(&mut self, name: &str) {
        match self.pinned.iter().position(|pin| pin == name) {
            Some(index) => {
                self.pinned.remove(index);
            }
            None => self.pinned.push(name.to_string()),
        }
    }

    pub fn pins(&self) -> &[String] {
        &self.pinned
    }

    pub fn set_pins(&mut self, pins: Vec<String>) {
        self.pinned = pins;
    }

    pub fn restore_pin(&mut self, name: &str, position: usize) {
        if !self.is_pinned(name) {
            let position = position.min(self.pinned.len());
            self.pinned.insert(position, name.to_string());
        }
    }

    pub fn retain_pins(&mut self, keep: impl Fn(&str) -> bool) -> bool {
        let before = self.pinned.len();
        self.pinned.retain(|pin| keep(pin));
        self.pinned.len() != before
    }

    pub fn solo(&mut self, name: &str, all: &[&str]) {
        self.toggle_hiding_all_but(all, |other| other == name);
    }

    pub fn pins_only(&mut self, all: &[&str]) {
        self.toggle_hiding_all_but(all, |_| false);
    }

    fn toggle_hiding_all_but(&mut self, all: &[&str], kept: impl Fn(&str) -> bool) {
        let target: HashSet<String> = all
            .iter()
            .filter(|&&other| !kept(other) && !self.is_pinned(other))
            .map(|&other| other.to_string())
            .collect();
        if self.hidden == target {
            self.hidden.clear();
        } else {
            self.hidden = target;
        }
    }

    pub fn is_visible(&self, name: &str, is_head: bool) -> bool {
        is_head || self.is_pinned(name) || !self.hidden.contains(name)
    }

    pub fn is_pinned(&self, name: &str) -> bool {
        self.pinned.iter().any(|pin| pin == name)
    }

    pub fn snapshot(&self) -> VisibilitySnapshot {
        VisibilitySnapshot {
            hidden: sorted(&self.hidden),
        }
    }

    pub fn restore(&mut self, snapshot: VisibilitySnapshot) {
        self.hidden = snapshot.hidden.into_iter().collect();
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
