use std::collections::HashMap;

use ravix::git::LineStats;
use ravix::review::{CommitStats, FileTotal, summarize};

fn commit(
    id: &'static str,
    author: &str,
    files: &[(&str, usize, usize)],
) -> CommitStats<&'static str> {
    CommitStats {
        id,
        author: author.to_string(),
        files: files
            .iter()
            .map(|(path, added, removed)| {
                (
                    path.to_string(),
                    LineStats {
                        added: *added,
                        removed: *removed,
                    },
                )
            })
            .collect::<HashMap<_, _>>(),
    }
}

fn sample() -> Vec<CommitStats<&'static str>> {
    vec![
        commit("c3", "ana", &[("src/lib.rs", 4, 1)]),
        commit("c2", "bo", &[("src/lib.rs", 10, 2), ("README", 1, 0)]),
        commit("c1", "ana", &[("src/main.rs", 3, 3)]),
    ]
}

#[test]
fn totals_add_up_every_own_commit() {
    let summary = summarize(&sample());

    assert_eq!(summary.commits, 3);
    assert_eq!((summary.added, summary.removed), (18, 6));
}

#[test]
fn authors_appear_once_in_order_of_first_sight() {
    assert_eq!(summarize(&sample()).authors, vec!["ana", "bo"]);
}

#[test]
fn files_are_unioned_with_their_own_sums_busiest_first() {
    let file = |path: &str, added, removed| FileTotal {
        path: path.to_string(),
        added,
        removed,
        commits: 0,
    };
    let summary = summarize(&sample());
    let files: Vec<FileTotal> = summary
        .files
        .into_iter()
        .map(|total| FileTotal {
            commits: 0,
            ..total
        })
        .collect();

    assert_eq!(
        files,
        vec![
            file("src/lib.rs", 14, 3),
            file("src/main.rs", 3, 3),
            file("README", 1, 0),
        ]
    );
}

#[test]
fn each_file_knows_how_many_commits_touched_it() {
    let summary = summarize(&sample());
    let lib = summary
        .files
        .iter()
        .find(|file| file.path == "src/lib.rs")
        .unwrap();

    assert_eq!(lib.commits, 2);
}

#[test]
fn a_file_filter_keeps_only_the_commits_that_touched_it() {
    let touching = ravix::review::commits_touching(&sample(), "src/lib.rs");

    assert_eq!(touching, vec!["c3", "c2"]);
}
