use ravix::pins::{PinSwitcherRow, switcher_rows};

fn pins(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn local(name: &str) -> String {
    name.to_string()
}

fn names(rows: &[PinSwitcherRow]) -> Vec<&str> {
    rows.iter().map(|row| row.name.as_str()).collect()
}

#[test]
fn pins_follow_their_most_recent_checkout() {
    let rows = switcher_rows(
        &pins(&["alpha", "beta", "gamma"]),
        None,
        &pins(&["gamma", "alpha", "beta"]),
        local,
    );

    assert_eq!(names(&rows), vec!["gamma", "alpha", "beta"]);
}

#[test]
fn pins_never_checked_out_trail_in_pin_order() {
    let rows = switcher_rows(
        &pins(&["zeta", "alpha", "beta"]),
        None,
        &pins(&["beta"]),
        local,
    );

    assert_eq!(names(&rows), vec!["beta", "zeta", "alpha"]);
}

#[test]
fn the_head_pin_leads_unnumbered_so_one_is_the_previous_pin() {
    let rows = switcher_rows(
        &pins(&["alpha", "beta", "gamma"]),
        Some("beta"),
        &pins(&["beta", "gamma", "alpha"]),
        local,
    );

    assert_eq!(names(&rows), vec!["beta", "gamma", "alpha"]);
    assert!(rows[0].is_head);
    assert_eq!(rows[0].number, None);
    assert_eq!(rows[1].number, Some(1));
    assert_eq!(rows[2].number, Some(2));
}

#[test]
fn only_the_first_nine_selectable_pins_get_a_number() {
    let many: Vec<String> = (1..=11).map(|n| format!("b{n:02}")).collect();

    let rows = switcher_rows(&many, None, &[], local);

    assert_eq!(rows[8].number, Some(9));
    assert_eq!(rows[9].number, None);
    assert_eq!(rows[10].number, None);
}

#[test]
fn a_remote_pin_ranks_by_checkouts_of_its_local_branch() {
    let as_local = |name: &str| name.strip_prefix("origin/").unwrap_or(name).to_string();

    let rows = switcher_rows(
        &pins(&["alpha", "origin/review"]),
        None,
        &pins(&["review", "alpha"]),
        as_local,
    );

    assert_eq!(names(&rows), vec!["origin/review", "alpha"]);
}

#[test]
fn an_unpinned_head_adds_no_here_row() {
    let rows = switcher_rows(&pins(&["alpha", "beta"]), Some("main"), &[], local);

    assert!(rows.iter().all(|row| !row.is_head));
    assert_eq!(rows[0].number, Some(1));
}

#[test]
fn a_remote_pin_of_the_head_branch_collapses_into_the_single_here_row() {
    let as_local = |name: &str| name.strip_prefix("origin/").unwrap_or(name).to_string();

    let rows = switcher_rows(&pins(&["x", "origin/x", "alpha"]), Some("x"), &[], as_local);

    assert_eq!(names(&rows), vec!["x", "alpha"]);
    assert_eq!(rows[1].number, Some(1));
}
