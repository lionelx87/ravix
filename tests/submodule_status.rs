mod common;

use common::{git, init_repo, submodule_fixture};
use ravix::git::Repo;
use ravix::submodule::SyncState;

#[test]
fn a_clean_checked_out_submodule_reports_synced_status() {
    let temp = tempfile::tempdir().unwrap();
    let main = submodule_fixture(temp.path());
    let head = git(&main.join("modules/sub"), &["rev-parse", "HEAD"])
        .trim()
        .to_string();

    let repo = Repo::discover(&main).unwrap();
    let subs = repo.submodules();

    assert_eq!(subs.len(), 1);
    let sub = &subs[0];
    assert_eq!(sub.name, "modules/sub");
    assert_eq!(sub.path, "modules/sub");
    assert_eq!(sub.url.as_deref(), Some("../sub-origin"));
    assert_eq!(sub.branch.as_deref(), Some("main"));
    assert_eq!(sub.checked_out.as_deref(), Some(head.as_str()));
    assert_eq!(sub.recorded.as_deref(), Some(head.as_str()));
    assert_eq!(sub.sync, SyncState::Synced);
    assert!(sub.initialized());
    assert_eq!(sub.dirty, 0);
    assert_eq!(sub.untracked, 0);
    assert_eq!(sub.last_commit.as_deref(), Some("feat: first version"));
}

#[test]
fn local_changes_are_counted_as_dirty_and_untracked() {
    let temp = tempfile::tempdir().unwrap();
    let main = submodule_fixture(temp.path());
    let sub_dir = main.join("modules/sub");
    std::fs::write(sub_dir.join("file.txt"), "changed").unwrap();
    std::fs::write(sub_dir.join("new.txt"), "new").unwrap();

    let repo = Repo::discover(&main).unwrap();
    let sub = &repo.submodules()[0];

    assert_eq!(sub.sync, SyncState::Synced);
    assert_eq!(sub.dirty, 1);
    assert_eq!(sub.untracked, 1);
    assert!(sub.is_dirty());
}

#[test]
fn a_submodule_on_another_commit_reports_drifted() {
    let temp = tempfile::tempdir().unwrap();
    let main = submodule_fixture(temp.path());
    let sub_dir = main.join("modules/sub");
    std::fs::write(sub_dir.join("file.txt"), "v2").unwrap();
    git(&sub_dir, &["add", "-A"]);
    git(&sub_dir, &["commit", "-qm", "feat: second version"]);
    let recorded = git(&main, &["rev-parse", ":modules/sub"]).trim().to_string();
    let checked_out = git(&sub_dir, &["rev-parse", "HEAD"]).trim().to_string();

    let repo = Repo::discover(&main).unwrap();
    let sub = &repo.submodules()[0];

    assert_eq!(sub.sync, SyncState::Drifted);
    assert_eq!(sub.recorded.as_deref(), Some(recorded.as_str()));
    assert_eq!(sub.checked_out.as_deref(), Some(checked_out.as_str()));
    assert_eq!(sub.last_commit.as_deref(), Some("feat: second version"));
}

#[test]
fn a_deinitialized_submodule_reports_uninitialized_with_url_and_recorded() {
    let temp = tempfile::tempdir().unwrap();
    let main = submodule_fixture(temp.path());
    let recorded = git(&main, &["rev-parse", ":modules/sub"]).trim().to_string();
    git(&main, &["submodule", "deinit", "-f", "-q", "modules/sub"]);

    let repo = Repo::discover(&main).unwrap();
    let sub = &repo.submodules()[0];

    assert_eq!(sub.sync, SyncState::Uninitialized);
    assert!(!sub.initialized());
    assert_eq!(sub.url.as_deref(), Some("../sub-origin"));
    assert_eq!(sub.recorded.as_deref(), Some(recorded.as_str()));
    assert_eq!(sub.checked_out, None);
    assert_eq!(sub.branch, None);
    assert_eq!(sub.dirty, 0);
    assert_eq!(sub.untracked, 0);
}

#[test]
fn nested_submodules_are_discovered_inside_an_entered_submodule() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let inner_origin = root.join("inner-origin");
    std::fs::create_dir(&inner_origin).unwrap();
    init_repo(&inner_origin);
    std::fs::write(inner_origin.join("lib.rs"), "pub fn f() {}").unwrap();
    git(&inner_origin, &["add", "-A"]);
    git(&inner_origin, &["commit", "-qm", "inner"]);

    let main = submodule_fixture(root);
    let sub_origin = root.join("sub-origin");
    git(
        &sub_origin,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            "../inner-origin",
            "inner",
        ],
    );
    git(&sub_origin, &["commit", "-qm", "add inner submodule"]);
    let sub_clone = main.join("modules/sub");
    git(&sub_clone, &["config", "protocol.file.allow", "always"]);
    git(&sub_clone, &["pull", "-q", "origin", "main"]);

    let repo = Repo::discover(main.join("modules/sub")).unwrap();
    let subs = repo.submodules();

    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].name, "inner");
    assert_eq!(subs[0].sync, SyncState::Uninitialized);
    assert!(subs[0].recorded.is_some());
}
