use crate::join::MergePrediction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullAction {
    UpToDate,
    FastForward,
    Choose,
}

pub fn pull_action(prediction: &MergePrediction) -> PullAction {
    match prediction {
        MergePrediction::UpToDate => PullAction::UpToDate,
        MergePrediction::FastForward => PullAction::FastForward,
        MergePrediction::Clean | MergePrediction::Conflicts { .. } => PullAction::Choose,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushState {
    NoUpstream,
    UpToDate,
    Ahead(usize),
    Diverged { ahead: usize, behind: usize },
}

pub fn push_state(tracking: Option<(usize, usize)>) -> PushState {
    let Some((ahead, behind)) = tracking else {
        return PushState::NoUpstream;
    };
    match (ahead, behind) {
        (0, _) => PushState::UpToDate,
        (ahead, 0) => PushState::Ahead(ahead),
        (ahead, behind) => PushState::Diverged { ahead, behind },
    }
}
