use ogma::graph::{GraphCommit, lay_out};

fn commit(id: &str, parents: &[&str]) -> GraphCommit<String> {
    GraphCommit {
        id: id.to_string(),
        parents: parents.iter().map(|p| p.to_string()).collect(),
    }
}

fn columns(rows: &[ogma::graph::GraphRow<String>]) -> Vec<usize> {
    rows.iter().map(|r| r.node_column).collect()
}

fn glyphs(rows: &[ogma::graph::GraphRow<String>]) -> Vec<&str> {
    rows.iter().map(|r| r.glyphs.as_str()).collect()
}

#[test]
fn linear_history_stays_in_one_lane() {
    let commits = [commit("a", &["b"]), commit("b", &["c"]), commit("c", &[])];

    let rows = lay_out(&commits);

    assert_eq!(columns(&rows), vec![0, 0, 0]);
    assert_eq!(glyphs(&rows), vec!["●", "●", "●"]);
}

#[test]
fn fork_and_merge_draw_curves_in_the_right_columns() {
    let commits = [
        commit("m", &["a", "b"]),
        commit("a", &["c"]),
        commit("b", &["c"]),
        commit("c", &[]),
    ];

    let rows = lay_out(&commits);

    assert_eq!(columns(&rows), vec![0, 0, 1, 0]);
    assert_eq!(glyphs(&rows), vec!["●─╮", "● │", "│ ●", "●─╯"]);
}

#[test]
fn a_side_lane_stays_open_across_several_commits() {
    let commits = [
        commit("m", &["a", "b"]),
        commit("a", &["d"]),
        commit("b", &["c"]),
        commit("c", &["d"]),
        commit("d", &[]),
    ];

    let rows = lay_out(&commits);

    assert_eq!(columns(&rows), vec![0, 0, 1, 1, 0]);
    assert_eq!(glyphs(&rows), vec!["●─╮", "● │", "│ ●", "│ ●", "●─╯"]);
}

#[test]
fn a_linear_line_shares_one_lane_key() {
    let commits = [commit("a", &["b"]), commit("b", &["c"]), commit("c", &[])];

    let rows = lay_out(&commits);

    assert_eq!(rows[0].node_key, rows[1].node_key, "one line, one key");
    assert_eq!(rows[1].node_key, rows[2].node_key);
}

#[test]
fn parallel_lines_carry_distinct_keys() {
    let commits = [
        commit("m", &["a", "b"]),
        commit("a", &["c"]),
        commit("b", &["c"]),
        commit("c", &[]),
    ];

    let rows = lay_out(&commits);

    assert_ne!(
        rows[1].node_key, rows[2].node_key,
        "the two parallel branch lines get different keys"
    );
}

#[test]
fn a_surviving_lines_key_is_unchanged_when_another_line_is_dropped() {
    let full = [
        commit("a", &["r"]),
        commit("b", &["r"]),
        commit("c", &["r"]),
        commit("r", &[]),
    ];
    let subset = [commit("a", &["r"]), commit("c", &["r"]), commit("r", &[])];

    let full_rows = lay_out(&full);
    let subset_rows = lay_out(&subset);

    let c_full = &full_rows[2];
    let c_subset = &subset_rows[1];
    assert_ne!(
        c_full.node_column, c_subset.node_column,
        "dropping b shifts c to a different column"
    );
    assert_eq!(
        c_full.node_key, c_subset.node_key,
        "but c's line keeps its key, so its color stays put"
    );
}
