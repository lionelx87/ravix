use ravix::remote::{Upstream, mismatched_upstream};

fn remotes(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn an_upstream_with_the_same_branch_name_matches() {
    assert_eq!(
        mismatched_upstream("main", "origin/main", &remotes(&["origin"])),
        None
    );
}

#[test]
fn an_upstream_with_another_branch_name_is_mismatched() {
    assert_eq!(
        mismatched_upstream("release/35.01.00", "origin/staging", &remotes(&["origin"])),
        Some(Upstream {
            remote: "origin".to_string(),
            branch: "staging".to_string(),
        })
    );
}

#[test]
fn the_same_branch_name_on_another_remote_still_matches() {
    assert_eq!(
        mismatched_upstream(
            "release/x",
            "upstream/release/x",
            &remotes(&["origin", "upstream"])
        ),
        None
    );
}

#[test]
fn branch_names_with_slashes_split_on_the_remote_name() {
    assert_eq!(
        mismatched_upstream(
            "feature/a",
            "my/fork/feature/b",
            &remotes(&["my", "my/fork"])
        ),
        Some(Upstream {
            remote: "my/fork".to_string(),
            branch: "feature/b".to_string(),
        })
    );
    assert_eq!(
        mismatched_upstream(
            "release/35.01.00",
            "origin/release/35.01.00",
            &remotes(&["origin"])
        ),
        None
    );
}

#[test]
fn an_upstream_on_an_unknown_remote_is_not_judged() {
    assert_eq!(
        mismatched_upstream("main", "gone/staging", &remotes(&["origin"])),
        None
    );
}
