use std::collections::HashMap;

use ravix::review::{base_candidates, nearest_base};

fn history() -> HashMap<&'static str, Vec<&'static str>> {
    [
        ("m1", vec![]),
        ("m2", vec!["m1"]),
        ("d1", vec!["m2"]),
        ("d2", vec!["d1"]),
        ("f1", vec!["d1"]),
        ("f2", vec!["f1"]),
    ]
    .into_iter()
    .collect()
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| name.to_string()).collect()
}

#[test]
fn the_trunk_the_branch_left_most_recently_wins() {
    let history = history();
    let candidates = [("main".to_string(), "m2"), ("develop".to_string(), "d2")];

    let base = nearest_base("f2", &candidates, |id| history[id].clone());

    assert_eq!(base.as_deref(), Some("develop"));
}

#[test]
fn a_tie_goes_to_the_earlier_candidate() {
    let history = history();
    let candidates = [("develop".to_string(), "d1"), ("main".to_string(), "d1")];

    let base = nearest_base("f2", &candidates, |id| history[id].clone());

    assert_eq!(base.as_deref(), Some("develop"));
}

#[test]
fn no_candidates_means_no_base() {
    let history = history();

    assert_eq!(nearest_base("f2", &[], |id| history[id].clone()), None);
}

#[test]
fn the_remote_default_leads_and_the_branch_and_its_upstream_are_excluded() {
    let existing = names(&["main", "origin/main", "develop", "origin/develop", "trunk"]);

    let candidates = base_candidates(
        "develop",
        Some("origin/develop"),
        Some("origin/main"),
        |name| existing.iter().any(|known| known == name),
    );

    assert_eq!(candidates, names(&["origin/main", "main", "trunk"]));
}
