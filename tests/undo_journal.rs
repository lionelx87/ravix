use ogma::undo::{InversePlan, UndoableAction, invert};

#[test]
fn staging_inverts_to_unstaging_and_back() {
    assert_eq!(
        invert(&UndoableAction::Staged("a.rs".into())),
        InversePlan::Unstage("a.rs".into())
    );
    assert_eq!(
        invert(&UndoableAction::Unstaged("a.rs".into())),
        InversePlan::Stage("a.rs".into())
    );
}

#[test]
fn hunk_and_bulk_staging_invert_symmetrically() {
    assert_eq!(
        invert(&UndoableAction::StagedHunk { patch: "P".into() }),
        InversePlan::UnstageHunk("P".into())
    );
    assert_eq!(
        invert(&UndoableAction::UnstagedHunk { patch: "P".into() }),
        InversePlan::StageHunk("P".into())
    );
    assert_eq!(invert(&UndoableAction::StagedAll), InversePlan::UnstageAll);
    assert_eq!(invert(&UndoableAction::UnstagedAll), InversePlan::StageAll);
}

#[test]
fn a_discard_inverts_to_restoring_the_snapshot() {
    assert_eq!(
        invert(&UndoableAction::Discarded {
            path: "src/main.rs".into(),
            snapshot: "deadbeef".into(),
        }),
        InversePlan::RestoreFile {
            path: "src/main.rs".into(),
            snapshot: "deadbeef".into(),
        }
    );
}

#[test]
fn a_commit_inverts_to_a_reflog_soft_reset() {
    assert_eq!(
        invert(&UndoableAction::Committed),
        InversePlan::ReflogSoftReset
    );
}

#[test]
fn a_checkout_inverts_to_returning_to_the_previous_head() {
    assert_eq!(
        invert(&UndoableAction::CheckedOut {
            previous: "main".into(),
        }),
        InversePlan::Checkout("main".into())
    );
}

#[test]
fn creating_a_branch_inverts_to_dropping_it_and_switching_back() {
    assert_eq!(
        invert(&UndoableAction::CreatedBranch {
            name: "feature".into(),
            previous: "main".into(),
        }),
        InversePlan::DropBranch {
            name: "feature".into(),
            back_to: "main".into(),
        }
    );
}

#[test]
fn deleting_a_branch_inverts_to_recreating_it_at_its_tip() {
    assert_eq!(
        invert(&UndoableAction::DeletedBranch {
            name: "feature".into(),
            oid: "deadbeef".into(),
        }),
        InversePlan::RestoreBranch {
            name: "feature".into(),
            oid: "deadbeef".into(),
        }
    );
}

#[test]
fn a_merge_inverts_to_resetting_head_back_to_the_previous_tip() {
    assert_eq!(
        invert(&UndoableAction::Merged {
            previous: "abc123".into(),
        }),
        InversePlan::ResetKeep("abc123".into())
    );
}

#[test]
fn a_cherry_pick_inverts_to_resetting_head_back_to_the_previous_tip() {
    assert_eq!(
        invert(&UndoableAction::CherryPicked {
            previous: "def456".into(),
        }),
        InversePlan::ResetKeep("def456".into())
    );
}
