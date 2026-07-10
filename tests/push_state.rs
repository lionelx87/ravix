use ogma::remote::{PushState, push_state};

#[test]
fn a_branch_without_an_upstream_has_nothing_to_track() {
    assert_eq!(push_state(None), PushState::NoUpstream);
}

#[test]
fn nothing_ahead_is_up_to_date() {
    assert_eq!(push_state(Some((0, 0))), PushState::UpToDate);
    assert_eq!(push_state(Some((0, 4))), PushState::UpToDate);
}

#[test]
fn commits_ahead_with_no_divergence_can_fast_forward_push() {
    assert_eq!(push_state(Some((2, 0))), PushState::Ahead(2));
}

#[test]
fn ahead_and_behind_means_a_diverged_push_needing_force() {
    assert_eq!(
        push_state(Some((2, 3))),
        PushState::Diverged {
            ahead: 2,
            behind: 3
        }
    );
}
