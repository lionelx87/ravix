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
    Diverged {
        ahead: usize,
        behind: usize,
    },
    Mismatched {
        upstream: Upstream,
        ahead: usize,
        behind: usize,
    },
}

pub fn is_non_fast_forward(error: &str) -> bool {
    error.contains("non-fast-forward")
        || error.contains("fetch first")
        || error.contains("Updates were rejected")
}

pub fn is_auth_failure(error: &str) -> bool {
    error.contains("Authentication failed")
        || error.contains("Invalid username or password")
        || error.contains("could not read Username")
        || error.contains("could not read Password")
        || error.contains("terminal prompts disabled")
        || error.contains("403 Forbidden")
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    pub remote: String,
    pub branch: String,
}

fn split_upstream(upstream: &str, remotes: &[String]) -> Option<Upstream> {
    remotes
        .iter()
        .filter_map(|remote| {
            let branch = upstream.strip_prefix(remote.as_str())?.strip_prefix('/')?;
            Some(Upstream {
                remote: remote.clone(),
                branch: branch.to_string(),
            })
        })
        .max_by_key(|candidate| candidate.remote.len())
}

pub fn mismatched_upstream(local: &str, upstream: &str, remotes: &[String]) -> Option<Upstream> {
    split_upstream(upstream, remotes).filter(|upstream| upstream.branch != local)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushTarget {
    Relink,
    IntoUpstream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushChoice {
    pub target: PushTarget,
    pub label: String,
    pub note: String,
    pub enabled: bool,
}

pub fn push_choices(
    local: &str,
    upstream: &Upstream,
    ahead: usize,
    behind: usize,
) -> Vec<PushChoice> {
    let remote = &upstream.remote;
    let into_note = match (ahead, behind) {
        (0, _) => "nothing to push into it".to_string(),
        (_, 0) => format!("{ahead} commit(s) land on it"),
        _ => format!("diverged ↑{ahead} ↓{behind}, never forced"),
    };
    vec![
        PushChoice {
            target: PushTarget::Relink,
            label: format!("Push to {remote}/{local} and relink"),
            note: "own name, becomes the upstream".to_string(),
            enabled: true,
        },
        PushChoice {
            target: PushTarget::IntoUpstream,
            label: format!("Push into {remote}/{}", upstream.branch),
            note: into_note,
            enabled: ahead > 0 && behind == 0,
        },
    ]
}

pub fn head_push_state(
    local: &str,
    upstream: Option<&str>,
    remotes: &[String],
    tracking: Option<(usize, usize)>,
) -> PushState {
    let mismatch = upstream.and_then(|upstream| mismatched_upstream(local, upstream, remotes));
    match mismatch {
        Some(upstream) => {
            let (ahead, behind) = tracking.unwrap_or((0, 0));
            PushState::Mismatched {
                upstream,
                ahead,
                behind,
            }
        }
        None => push_state(tracking),
    }
}
