use ravix::remote::{PushTarget, Upstream, push_choices};

fn staging() -> Upstream {
    Upstream {
        remote: "origin".to_string(),
        branch: "staging".to_string(),
    }
}

fn enabled(ahead: usize, behind: usize) -> Vec<(PushTarget, bool)> {
    push_choices("release/x", &staging(), ahead, behind)
        .into_iter()
        .map(|choice| (choice.target, choice.enabled))
        .collect()
}

#[test]
fn relinking_comes_first_and_names_both_destinations() {
    let choices = push_choices("release/x", &staging(), 2, 0);

    assert_eq!(choices[0].target, PushTarget::Relink);
    assert!(choices[0].label.contains("origin/release/x"));
    assert_eq!(choices[1].target, PushTarget::IntoUpstream);
    assert!(choices[1].label.contains("origin/staging"));
}

#[test]
fn pushing_into_the_upstream_needs_commits_strictly_ahead() {
    assert_eq!(
        enabled(2, 0),
        vec![(PushTarget::Relink, true), (PushTarget::IntoUpstream, true)]
    );
}

#[test]
fn nothing_ahead_still_allows_publishing_under_its_own_name() {
    assert_eq!(
        enabled(0, 0),
        vec![
            (PushTarget::Relink, true),
            (PushTarget::IntoUpstream, false)
        ]
    );
    assert_eq!(
        enabled(0, 3),
        vec![
            (PushTarget::Relink, true),
            (PushTarget::IntoUpstream, false)
        ]
    );
}

#[test]
fn a_diverged_upstream_is_never_pushed_into() {
    assert_eq!(
        enabled(2, 3),
        vec![
            (PushTarget::Relink, true),
            (PushTarget::IntoUpstream, false)
        ]
    );
}

#[test]
fn a_blocked_choice_explains_why() {
    let nothing = push_choices("release/x", &staging(), 0, 0);
    let diverged = push_choices("release/x", &staging(), 2, 3);

    assert!(nothing[1].note.contains("nothing"), "{}", nothing[1].note);
    assert!(
        diverged[1].note.contains("diverged"),
        "{}",
        diverged[1].note
    );
}
