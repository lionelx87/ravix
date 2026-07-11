use ravix::remote::{PushState, push_state};

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

#[test]
fn a_rejected_push_is_recognised_as_non_fast_forward() {
    let rejection = " ! [rejected]        main -> main (non-fast-forward)\n\
        error: failed to push some refs to 'origin'\n\
        hint: Updates were rejected because the tip of your current branch is behind";
    assert!(ravix::remote::is_non_fast_forward(rejection));

    assert!(!ravix::remote::is_non_fast_forward(
        "fatal: unable to access 'origin': Could not resolve host"
    ));
}
