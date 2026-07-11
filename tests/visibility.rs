use ravix::visibility::Visibility;

#[test]
fn everything_is_visible_by_default() {
    let visibility = Visibility::default();

    assert!(visibility.is_visible("main", true));
    assert!(visibility.is_visible("feature", false));
    assert!(visibility.is_visible("anything", false));
}

#[test]
fn snapshot_captures_sorted_state_and_restore_reapplies_it() {
    let mut visibility = Visibility::default();
    visibility.toggle("main");
    visibility.toggle("alpha");
    visibility.pin("feature/x");

    let snapshot = visibility.snapshot();
    assert_eq!(
        snapshot.hidden,
        vec!["alpha".to_string(), "main".to_string()],
        "snapshot is sorted for deterministic output"
    );
    assert_eq!(snapshot.pinned, vec!["feature/x".to_string()]);

    let mut restored = Visibility::default();
    restored.restore(snapshot);
    assert!(!restored.is_visible("alpha", false));
    assert!(!restored.is_visible("main", false));
    assert!(restored.is_pinned("feature/x"));
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

    visibility.pin("bugfix");
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
