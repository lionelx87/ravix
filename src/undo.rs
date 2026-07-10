#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoableAction {
    Staged(String),
    Unstaged(String),
    StagedHunk { patch: String },
    UnstagedHunk { patch: String },
    StagedAll,
    UnstagedAll,
    Discarded { path: String, snapshot: String },
    Committed,
    CheckedOut { previous: String },
    CreatedBranch { name: String, previous: String },
    DeletedBranch { name: String, oid: String },
    Merged { previous: String },
    CherryPicked { previous: String },
    Rebased { previous: String },
    Stashed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InversePlan {
    Stage(String),
    Unstage(String),
    StageHunk(String),
    UnstageHunk(String),
    StageAll,
    UnstageAll,
    RestoreFile { path: String, snapshot: String },
    ReflogSoftReset,
    Checkout(String),
    DropBranch { name: String, back_to: String },
    RestoreBranch { name: String, oid: String },
    ResetKeep(String),
    StashPop,
}

pub fn invert(action: &UndoableAction) -> InversePlan {
    match action {
        UndoableAction::Staged(path) => InversePlan::Unstage(path.clone()),
        UndoableAction::Unstaged(path) => InversePlan::Stage(path.clone()),
        UndoableAction::StagedHunk { patch } => InversePlan::UnstageHunk(patch.clone()),
        UndoableAction::UnstagedHunk { patch } => InversePlan::StageHunk(patch.clone()),
        UndoableAction::StagedAll => InversePlan::UnstageAll,
        UndoableAction::UnstagedAll => InversePlan::StageAll,
        UndoableAction::Discarded { path, snapshot } => InversePlan::RestoreFile {
            path: path.clone(),
            snapshot: snapshot.clone(),
        },
        UndoableAction::Committed => InversePlan::ReflogSoftReset,
        UndoableAction::CheckedOut { previous } => InversePlan::Checkout(previous.clone()),
        UndoableAction::CreatedBranch { name, previous } => InversePlan::DropBranch {
            name: name.clone(),
            back_to: previous.clone(),
        },
        UndoableAction::DeletedBranch { name, oid } => InversePlan::RestoreBranch {
            name: name.clone(),
            oid: oid.clone(),
        },
        UndoableAction::Merged { previous } => InversePlan::ResetKeep(previous.clone()),
        UndoableAction::CherryPicked { previous } => InversePlan::ResetKeep(previous.clone()),
        UndoableAction::Rebased { previous } => InversePlan::ResetKeep(previous.clone()),
        UndoableAction::Stashed => InversePlan::StashPop,
    }
}
