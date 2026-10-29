use std::collections::HashSet;

use ravix::bench::{PickRef, Prediction, RowKind, application_order, chain, classify};
use ravix::review::ReviewRow;

#[test]
fn own_commits_are_pickable_unless_merged_or_already_in_the_destination() {
    let rows = vec![
        ReviewRow::Commit("f3"),
        ReviewRow::Merge("fm"),
        ReviewRow::Commit("f2"),
        ReviewRow::Commit("f1"),
    ];
    let missing_from_destination: HashSet<&str> = ["f3", "f1"].into_iter().collect();

    assert_eq!(
        classify(&rows, &missing_from_destination),
        vec![
            ("f3", RowKind::Pickable),
            ("fm", RowKind::Merge),
            ("f2", RowKind::AlreadyInDestination),
            ("f1", RowKind::Pickable),
        ]
    );
}

fn pick(id: &'static str, branch: &str, depth: usize, time: i64) -> PickRef<&'static str> {
    PickRef {
        id,
        branch: branch.to_string(),
        depth,
        time,
    }
}

#[test]
fn picks_apply_oldest_first_within_a_branch_and_by_date_across_branches() {
    let picks = vec![
        pick("a-new", "feature/a", 0, 300),
        pick("b-only", "feature/b", 2, 200),
        pick("a-old", "feature/a", 3, 400),
    ];

    assert_eq!(
        application_order(&picks),
        vec!["b-only", "a-old", "a-new"],
        "a branch's older commit goes first even when its date is later"
    );
}

#[test]
fn each_pick_lands_on_the_previous_one_and_a_conflict_leaves_the_rest_pending() {
    let mut seen = Vec::new();
    let replay = chain("tip".to_string(), &["p1", "p2", "p3"], |onto, pick| {
        seen.push(format!("{pick} onto {onto}"));
        match *pick {
            "p2" => Err(vec!["src/login.rs".to_string()]),
            _ => Ok(format!("{pick}'")),
        }
    });

    assert_eq!(seen, vec!["p1 onto tip", "p2 onto p1'"]);
    assert_eq!(
        replay.predictions,
        vec![
            Prediction::Clean,
            Prediction::Conflicts(vec!["src/login.rs".to_string()]),
            Prediction::Pending,
        ]
    );
    assert_eq!(replay.tip, None, "a conflicting chain has no clean result");
}

#[test]
fn a_fully_clean_chain_ends_on_the_last_replayed_commit() {
    let replay = chain(0, &[1, 2], |onto, pick| Ok(onto * 10 + pick));

    assert_eq!(
        replay.predictions,
        vec![Prediction::Clean, Prediction::Clean]
    );
    assert_eq!(replay.tip, Some(12));
}
