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
    }
}
