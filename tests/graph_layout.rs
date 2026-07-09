use ogma::graph::{GraphCommit, lay_out};

fn commit(id: &str, parents: &[&str]) -> GraphCommit<String> {
    GraphCommit {
        id: id.to_string(),
        parents: parents.iter().map(|p| p.to_string()).collect(),
    }
}

fn columns(rows: &[ogma::graph::GraphRow]) -> Vec<usize> {
    rows.iter().map(|r| r.node_column).collect()
}

fn glyphs(rows: &[ogma::graph::GraphRow]) -> Vec<&str> {
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
