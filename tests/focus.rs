use ogma::focus::{FocusSet, parse, remove, serialize, upsert};
use ogma::visibility::VisibilitySnapshot;

fn set(name: &str, hidden: &[&str], pinned: &[&str]) -> FocusSet {
    FocusSet {
        name: name.to_string(),
        state: VisibilitySnapshot {
            hidden: hidden.iter().map(|s| s.to_string()).collect(),
            pinned: pinned.iter().map(|s| s.to_string()).collect(),
        },
    }
}

#[test]
fn a_set_round_trips_through_text() {
    let sets = vec![set(
        "feature-work",
        &["main", "release/v1"],
        &["feature/login"],
    )];

    let parsed = parse(&serialize(&sets));

    assert_eq!(parsed, sets);
}

#[test]
fn upsert_replaces_a_same_named_set_in_place_or_appends() {
    let mut sets = vec![set("a", &["x"], &[]), set("b", &["y"], &[])];

    upsert(&mut sets, set("a", &["z"], &["p"]));
    assert_eq!(
        sets,
        vec![set("a", &["z"], &["p"]), set("b", &["y"], &[])],
        "same name replaces in place, no duplicate"
    );

    upsert(&mut sets, set("c", &[], &[]));
    assert_eq!(sets.len(), 3, "a new name appends");
    assert_eq!(sets[2].name, "c");
}

#[test]
fn remove_drops_a_set_by_name_and_ignores_missing() {
    let mut sets = vec![set("a", &[], &[]), set("b", &[], &[])];

    remove(&mut sets, "a");
    assert_eq!(sets, vec![set("b", &[], &[])]);

    remove(&mut sets, "gone");
    assert_eq!(sets.len(), 1, "removing a missing name is a no-op");
}

#[test]
fn parse_ignores_blank_and_stray_lines() {
    let text = "\nhide orphan\ngarbage line\nfocus real\nhide main\npin keep\n";

    let parsed = parse(text);

    assert_eq!(
        parsed,
        vec![set("real", &["main"], &["keep"])],
        "a hide/pin before any focus, and junk lines, are ignored"
    );
}
