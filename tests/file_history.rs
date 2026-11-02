use ravix::history::{
    HistoryRow, Origin, Rev, Tip, Version, change_bar, compared, parse_log, start_of,
};

const SEP: char = '\u{1f}';
const START: char = '\u{1e}';

fn header(id: &str, parents: &str, author: &str, time: i64, subject: &str) -> String {
    format!("{START}{id}{SEP}{parents}{SEP}{author}{SEP}{time}{SEP}{subject}\0")
}

#[test]
fn a_plain_record_becomes_a_row_with_its_author_and_the_file_change() {
    let log = format!(
        "{}\n3\t1\tsrc/app.rs\0",
        header("c2", "c1", "Ana", 1_700_000_000, "fix nav")
    );

    let rows = parse_log(&log);

    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.id, Some("c2".to_string()));
    assert_eq!(row.parents, vec!["c1".to_string()]);
    assert_eq!(row.author, "Ana");
    assert_eq!(row.time, 1_700_000_000);
    assert_eq!(row.subject, "fix nav");
    assert_eq!(row.path, "src/app.rs");
    assert_eq!(row.renamed_from, None);
    assert_eq!((row.added, row.removed), (3, 1));
    assert!(!row.binary);
}

#[test]
fn a_rename_keeps_the_new_path_and_remembers_the_old_one() {
    let log = format!(
        "{}\n0\t0\t\0src/old.rs\0src/new.rs\0{}\n2\t0\tsrc/old.rs\0",
        header("c2", "c1", "Lio", 2, "rename"),
        header("c1", "", "Ana", 1, "create"),
    );

    let rows = parse_log(&log);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].path, "src/new.rs");
    assert_eq!(rows[0].renamed_from.as_deref(), Some("src/old.rs"));
    assert_eq!(rows[1].path, "src/old.rs");
    assert!(rows[1].parents.is_empty());
}

#[test]
fn a_merge_lists_every_parent_and_a_binary_change_has_no_line_counts() {
    let log = format!(
        "{}\n1\t1\tsrc/new.rs\0{}\n-\t-\tlogo.png\0",
        header("m", "p1 p2", "Ana", 3, "Merge branch 'side'"),
        header("b", "p1", "Ana", 2, "logo"),
    );

    let rows = parse_log(&log);

    assert_eq!(rows[0].parents, vec!["p1".to_string(), "p2".to_string()]);
    assert!(rows[1].binary);
    assert_eq!((rows[1].added, rows[1].removed), (0, 0));
}

#[test]
fn from_head_the_history_starts_at_head_and_shows_uncommitted_changes_only_when_dirty() {
    let dirty = start_of(Origin::<&str>::Head, |_| true, true);
    assert_eq!(dirty.tip, Tip::Head);
    assert_eq!(dirty.preselect, None);
    assert!(dirty.uncommitted);

    let clean = start_of(Origin::<&str>::Head, |_| true, false);
    assert!(!clean.uncommitted);
}

#[test]
fn a_commit_reachable_from_head_starts_at_head_with_that_commit_preselected() {
    let start = start_of(Origin::Commit("old"), |_| true, true);

    assert_eq!(start.tip, Tip::Head);
    assert_eq!(start.preselect, Some("old"));
    assert!(start.uncommitted);
}

#[test]
fn a_commit_on_another_branch_starts_there_without_uncommitted_changes() {
    let start = start_of(Origin::Commit("elsewhere"), |_| false, true);

    assert_eq!(start.tip, Tip::Commit("elsewhere"));
    assert_eq!(start.preselect, Some("elsewhere"));
    assert!(!start.uncommitted);
}

#[test]
fn the_change_bar_is_relative_to_the_largest_change_and_never_hides_a_small_one() {
    assert_eq!(change_bar(120, 120, 8), 8);
    assert_eq!(change_bar(60, 120, 8), 4);
    assert_eq!(change_bar(1, 120, 8), 1);
    assert_eq!(change_bar(0, 120, 8), 0);
}

#[test]
fn the_change_bar_shrinks_when_a_bigger_change_streams_in() {
    assert_eq!(change_bar(40, 40, 8), 8);
    assert_eq!(change_bar(40, 80, 8), 4);
}

fn row(
    id: Option<&'static str>,
    path: &str,
    renamed_from: Option<&str>,
) -> HistoryRow<&'static str> {
    HistoryRow {
        id,
        parents: Vec::new(),
        author: String::new(),
        time: 0,
        subject: String::new(),
        path: path.to_string(),
        renamed_from: renamed_from.map(str::to_string),
        added: 0,
        removed: 0,
        binary: false,
    }
}

fn version(rev: Rev<&'static str>, path: &str) -> Version<&'static str> {
    Version {
        rev,
        path: path.to_string(),
    }
}

fn file_rows() -> Vec<HistoryRow<&'static str>> {
    vec![
        row(None, "src/new.rs", None),
        row(Some("c3"), "src/new.rs", None),
        row(Some("c2"), "src/new.rs", Some("src/old.rs")),
        row(Some("c1"), "src/old.rs", None),
    ]
}

#[test]
fn without_an_anchor_a_commit_shows_its_own_change_from_its_first_parent() {
    let rows = file_rows();

    assert_eq!(
        compared(&rows, 1, None),
        Some((
            version(Rev::FirstParent("c3"), "src/new.rs"),
            version(Rev::Commit("c3"), "src/new.rs"),
        ))
    );
    assert_eq!(
        compared(&rows, 2, None),
        Some((
            version(Rev::FirstParent("c2"), "src/old.rs"),
            version(Rev::Commit("c2"), "src/new.rs"),
        )),
        "a rename reads its parent's side under the old path"
    );
}

#[test]
fn without_an_anchor_the_uncommitted_row_compares_head_with_the_working_tree() {
    assert_eq!(
        compared(&file_rows(), 0, None),
        Some((
            version(Rev::Head, "src/new.rs"),
            version(Rev::Worktree, "src/new.rs"),
        ))
    );
}

#[test]
fn an_anchor_compares_the_two_versions_oldest_first_whichever_was_marked_first() {
    let rows = file_rows();
    let expected = Some((
        version(Rev::Commit("c1"), "src/old.rs"),
        version(Rev::Commit("c3"), "src/new.rs"),
    ));

    assert_eq!(compared(&rows, 1, Some(3)), expected);
    assert_eq!(compared(&rows, 3, Some(1)), expected);
}

#[test]
fn an_anchor_against_the_uncommitted_row_reaches_the_working_tree() {
    assert_eq!(
        compared(&file_rows(), 0, Some(2)),
        Some((
            version(Rev::Commit("c2"), "src/new.rs"),
            version(Rev::Worktree, "src/new.rs"),
        ))
    );
}

#[test]
fn an_anchor_on_the_selected_row_shows_that_row_own_change() {
    let rows = file_rows();

    assert_eq!(compared(&rows, 1, Some(1)), compared(&rows, 1, None));
    assert_eq!(compared(&rows, 7, None), None);
}
