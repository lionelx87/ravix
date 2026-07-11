use ravix::drag::{DropIntent, RowRef, resolve_drop};

fn rows() -> Vec<RowRef> {
    vec![
        RowRef {
            oid: "aaa".into(),
            branch: Some("main".into()),
        },
        RowRef {
            oid: "bbb".into(),
            branch: Some("feature".into()),
        },
        RowRef {
            oid: "ccc".into(),
            branch: None,
        },
    ]
}

#[test]
fn dragging_a_branch_onto_another_row_yields_an_intent() {
    let intent = resolve_drop(&rows(), 1, 0);

    assert_eq!(
        intent,
        Some(DropIntent {
            source_branch: "feature".into(),
            target_oid: "aaa".into(),
            target_branch: Some("main".into()),
        })
    );
}

#[test]
fn dropping_onto_a_branchless_commit_carries_no_target_branch() {
    let intent = resolve_drop(&rows(), 1, 2);

    assert_eq!(
        intent,
        Some(DropIntent {
            source_branch: "feature".into(),
            target_oid: "ccc".into(),
            target_branch: None,
        })
    );
}

#[test]
fn releasing_on_the_same_row_is_not_a_drop() {
    assert_eq!(resolve_drop(&rows(), 1, 1), None);
}

#[test]
fn a_drag_that_does_not_start_on_a_branch_is_not_a_drop() {
    assert_eq!(resolve_drop(&rows(), 2, 0), None);
}

#[test]
fn out_of_range_rows_are_not_a_drop() {
    assert_eq!(resolve_drop(&rows(), 1, 9), None);
    assert_eq!(resolve_drop(&rows(), 9, 0), None);
}
