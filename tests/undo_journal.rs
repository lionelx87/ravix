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
