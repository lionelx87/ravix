use ravix::visibility::Visibility;

#[test]
fn everything_is_visible_by_default() {
    let visibility = Visibility::default();

    assert!(visibility.is_visible("main", true));
    assert!(visibility.is_visible("feature", false));
    assert!(visibility.is_visible("anything", false));
}

#[test]
fn snapshot_captures_sorted_hidden_state_and_restore_reapplies_it() {
    let mut visibility = Visibility::default();
    visibility.toggle("main");
    visibility.toggle("alpha");

    let snapshot = visibility.snapshot();
    assert_eq!(
        snapshot.hidden,
        vec!["alpha".to_string(), "main".to_string()],
        "snapshot is sorted for deterministic output"
    );

    let mut restored = Visibility::default();
    restored.restore(snapshot);
    assert!(!restored.is_visible("alpha", false));
    assert!(!restored.is_visible("main", false));
}

#[test]
fn restoring_a_snapshot_leaves_the_pins_alone() {
    let mut saved = Visibility::default();
    saved.toggle_pin("old-pin");
    let snapshot = saved.snapshot();

    let mut visibility = Visibility::default();
    visibility.toggle_pin("mine");
    visibility.restore(snapshot);

    assert_eq!(visibility.pins(), ["mine"]);
}

#[test]
fn toggle_hides_then_reshows_a_branch() {
    let mut visibility = Visibility::default();

    visibility.toggle("feature");
    assert!(
        !visibility.is_visible("feature", false),
        "hidden after toggle"
    );
    assert!(visibility.is_visible("main", false), "others untouched");

    visibility.toggle("feature");
    assert!(
        visibility.is_visible("feature", false),
        "shown after second toggle"
    );
}

#[test]
fn a_pinned_branch_survives_a_solo() {
    let mut visibility = Visibility::default();
    let all = ["main", "feature", "bugfix"];

    visibility.toggle_pin("bugfix");
    visibility.solo("feature", &all);

    assert!(
        visibility.is_visible("feature", false),
        "the soloed branch stays"
    );
    assert!(
        visibility.is_visible("bugfix", false),
        "the pinned branch survives"
    );
    assert!(!visibility.is_visible("main", false), "the rest are hidden");
}

#[test]
fn solo_hides_every_branch_but_the_focused_one() {
    let mut visibility = Visibility::default();
    let all = ["main", "feature", "bugfix"];

    visibility.solo("feature", &all);

    assert!(
        visibility.is_visible("feature", false),
        "the soloed branch stays"
    );
    assert!(!visibility.is_visible("main", false), "others are hidden");
    assert!(!visibility.is_visible("bugfix", false), "others are hidden");
}

#[test]
fn the_head_branch_stays_visible_even_when_hidden() {
    let mut visibility = Visibility::default();

    visibility.toggle("main");

    assert!(
        visibility.is_visible("main", true),
        "HEAD's branch is never hideable"
    );
    assert!(
        !visibility.is_visible("main", false),
        "the same name hidden elsewhere stays hidden"
    );
}

#[test]
fn pins_keep_the_order_they_were_pinned_in() {
    let mut visibility = Visibility::default();

    visibility.toggle_pin("zeta");
    visibility.toggle_pin("alpha");
    visibility.toggle_pin("mid");
    visibility.toggle_pin("alpha");

    assert_eq!(
        visibility.pins(),
        ["zeta", "mid"],
        "unpinning drops in place"
    );
}

#[test]
fn pins_only_hides_every_unpinned_branch_and_toggles_back() {
    let mut visibility = Visibility::default();
    let all = ["main", "feature", "bugfix"];
    visibility.toggle_pin("feature");

    visibility.pins_only(&all);
    assert!(visibility.is_visible("feature", false), "the pin stays");
    assert!(
        !visibility.is_visible("bugfix", false),
        "unpinned are hidden"
    );
    assert!(visibility.is_visible("main", true), "HEAD stays");

    visibility.pins_only(&all);
    assert!(visibility.is_visible("bugfix", false), "again shows all");
}

#[test]
fn a_restored_pin_returns_to_its_original_position() {
    let mut visibility = Visibility::default();
    visibility.toggle_pin("first");
    visibility.toggle_pin("third");

    visibility.restore_pin("second", 1);

    assert_eq!(visibility.pins(), ["first", "second", "third"]);
}
