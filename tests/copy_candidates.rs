use ravix::copy::branch_candidates;
use ravix::git::{BadgeKind, RefBadge};

fn badge(label: &str, kind: BadgeKind) -> RefBadge {
    RefBadge {
        label: label.to_string(),
        kind,
    }
}

#[test]
fn the_current_branch_leads_then_locals_then_remotes() {
    let badges = [
        badge("origin/main", BadgeKind::Upstream),
        badge("feature", BadgeKind::LocalBranch),
        badge("main", BadgeKind::CurrentBranch),
    ];

    assert_eq!(
        branch_candidates(&badges),
        vec!["main", "feature", "origin/main"]
    );
}

#[test]
fn a_shared_upstream_is_offered_once() {
    let badges = [
        badge("main", BadgeKind::LocalBranch),
        badge("origin/main", BadgeKind::Upstream),
        badge("trunk", BadgeKind::LocalBranch),
        badge("origin/main", BadgeKind::Upstream),
    ];

    assert_eq!(
        branch_candidates(&badges),
        vec!["main", "trunk", "origin/main"]
    );
}

#[test]
fn a_detached_head_is_not_a_branch() {
    let badges = [badge("HEAD", BadgeKind::Head)];

    assert!(branch_candidates(&badges).is_empty());
}

#[test]
fn a_repeated_upstream_is_offered_once_even_when_not_adjacent() {
    let badges = [
        badge("origin/main", BadgeKind::Upstream),
        badge("origin/dev", BadgeKind::Upstream),
        badge("origin/main", BadgeKind::Upstream),
    ];

    assert_eq!(
        branch_candidates(&badges),
        vec!["origin/main", "origin/dev"]
    );
}

#[test]
fn a_commit_without_badges_offers_nothing() {
    assert!(branch_candidates(&[]).is_empty());
}

#[test]
fn a_single_local_branch_is_offered_alone() {
    let badges = [badge("feature", BadgeKind::LocalBranch)];

    assert_eq!(branch_candidates(&badges), vec!["feature"]);
}
