use crate::git::{BadgeKind, RefBadge};

pub fn branch_candidates(badges: &[RefBadge]) -> Vec<String> {
    let rank = |kind: BadgeKind| match kind {
        BadgeKind::CurrentBranch => Some(0),
        BadgeKind::LocalBranch => Some(1),
        BadgeKind::Upstream => Some(2),
        BadgeKind::Head => None,
    };
    let mut ranked: Vec<(usize, &str)> = badges
        .iter()
        .filter_map(|badge| Some((rank(badge.kind)?, badge.label.as_str())))
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    let mut candidates: Vec<String> = Vec::new();
    for (_, label) in ranked {
        if !candidates.iter().any(|seen| seen == label) {
            candidates.push(label.to_string());
        }
    }
    candidates
}
