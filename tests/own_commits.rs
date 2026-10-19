use std::collections::HashMap;

use ravix::review::{ReviewRow, own_commits};

struct History(HashMap<&'static str, Vec<&'static str>>);

impl History {
    fn new(edges: &[(&'static str, &[&'static str])]) -> Self {
        History(
            edges
                .iter()
                .map(|(id, parents)| (*id, parents.to_vec()))
                .collect(),
        )
    }

    fn parents(&self, id: &&'static str) -> Vec<&'static str> {
        self.0.get(id).cloned().unwrap_or_default()
    }
}

fn rows(history: &History, tip: &'static str, base: &'static str) -> Vec<ReviewRow<&'static str>> {
    own_commits(tip, base, |id| history.parents(id))
}

#[test]
fn a_linear_branch_lists_its_commits_newest_first() {
    let history = History::new(&[
        ("m1", &[]),
        ("m2", &["m1"]),
        ("f1", &["m1"]),
        ("f2", &["f1"]),
    ]);

    assert_eq!(
        rows(&history, "f2", "m2"),
        vec![ReviewRow::Commit("f2"), ReviewRow::Commit("f1")]
    );
}

#[test]
fn merging_the_base_back_in_leaves_only_a_marker() {
    let history = History::new(&[
        ("m1", &[]),
        ("m2", &["m1"]),
        ("m3", &["m2"]),
        ("f1", &["m1"]),
        ("fm", &["f1", "m2"]),
        ("f2", &["fm"]),
    ]);

    assert_eq!(
        rows(&history, "f2", "m3"),
        vec![
            ReviewRow::Commit("f2"),
            ReviewRow::Merge("fm"),
            ReviewRow::Commit("f1"),
        ]
    );
}

#[test]
fn a_branch_already_merged_into_its_base_keeps_its_commits() {
    let history = History::new(&[
        ("m1", &[]),
        ("m2", &["m1"]),
        ("f1", &["m1"]),
        ("f2", &["f1"]),
        ("mm", &["m2", "f2"]),
        ("m3", &["mm"]),
    ]);

    assert_eq!(
        rows(&history, "f2", "m3"),
        vec![ReviewRow::Commit("f2"), ReviewRow::Commit("f1")]
    );
}

#[test]
fn a_fast_forwarded_branch_has_no_own_commits() {
    let history = History::new(&[("m1", &[]), ("m2", &["m1"]), ("m3", &["m2"])]);

    assert_eq!(rows(&history, "m2", "m3"), vec![]);
}

#[test]
fn a_sibling_merged_in_is_left_out() {
    let history = History::new(&[
        ("m1", &[]),
        ("y1", &["m1"]),
        ("f1", &["m1"]),
        ("fy", &["f1", "y1"]),
        ("f2", &["fy"]),
    ]);

    assert_eq!(
        rows(&history, "f2", "m1"),
        vec![
            ReviewRow::Commit("f2"),
            ReviewRow::Merge("fy"),
            ReviewRow::Commit("f1"),
        ]
    );
}
