use ravix::staging::{SplitRow, split_rows};

fn row(left: Option<&str>, right: Option<&str>) -> SplitRow {
    SplitRow {
        left: left.map(String::from),
        right: right.map(String::from),
    }
}

fn lines(raw: &[&str]) -> Vec<String> {
    raw.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_context_line_appears_on_both_sides() {
    let rows = split_rows(&lines(&[" unchanged"]));

    assert_eq!(rows, vec![row(Some(" unchanged"), Some(" unchanged"))]);
}

#[test]
fn a_removed_run_pairs_with_the_added_run_by_offset() {
    let rows = split_rows(&lines(&["-old a", "-old b", "+new a", "+new b"]));

    assert_eq!(
        rows,
        vec![
            row(Some("-old a"), Some("+new a")),
            row(Some("-old b"), Some("+new b")),
        ]
    );
}

#[test]
fn unequal_runs_pad_the_shorter_side_with_none() {
    let more_removed = split_rows(&lines(&["-a", "-b", "+x"]));
    assert_eq!(
        more_removed,
        vec![row(Some("-a"), Some("+x")), row(Some("-b"), None)],
        "an extra removed line has no right counterpart"
    );

    let more_added = split_rows(&lines(&["-a", "+x", "+y"]));
    assert_eq!(
        more_added,
        vec![row(Some("-a"), Some("+x")), row(None, Some("+y"))],
        "an extra added line has no left counterpart"
    );
}

#[test]
fn add_only_and_delete_only_hunks_fill_one_side() {
    let added = split_rows(&lines(&["+x", "+y"]));
    assert_eq!(added, vec![row(None, Some("+x")), row(None, Some("+y"))]);

    let removed = split_rows(&lines(&["-a", "-b"]));
    assert_eq!(removed, vec![row(Some("-a"), None), row(Some("-b"), None)]);
}
