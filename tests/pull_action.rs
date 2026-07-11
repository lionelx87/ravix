use ravix::join::MergePrediction;
use ravix::remote::{PullAction, pull_action};

#[test]
fn an_upstream_already_in_head_is_up_to_date() {
    assert_eq!(
        pull_action(&MergePrediction::UpToDate),
        PullAction::UpToDate
    );
}

#[test]
fn a_fast_forward_pull_moves_forward_automatically() {
    assert_eq!(
        pull_action(&MergePrediction::FastForward),
        PullAction::FastForward
    );
}

#[test]
fn a_clean_but_diverged_pull_asks_you_to_choose() {
    assert_eq!(pull_action(&MergePrediction::Clean), PullAction::Choose);
}

#[test]
fn a_conflicting_pull_also_asks_you_to_choose() {
    assert_eq!(
        pull_action(&MergePrediction::Conflicts {
            files: vec!["app.rs".into()]
        }),
        PullAction::Choose
    );
}
