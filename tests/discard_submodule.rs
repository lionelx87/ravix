use std::path::Path;
use std::process::Command;

use ravix::mutate::GitCli;

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "test"]);
}

#[test]
fn discard_reverts_a_submodule_pointer_moved_by_new_commits() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let sub_origin = root.join("sub-origin");
    std::fs::create_dir(&sub_origin).unwrap();
    init_repo(&sub_origin);
    std::fs::write(sub_origin.join("file.txt"), "v1").unwrap();
    git(&sub_origin, &["add", "-A"]);
    git(&sub_origin, &["commit", "-qm", "c1"]);
    let first = git(&sub_origin, &["rev-parse", "HEAD"]).trim().to_string();
    std::fs::write(sub_origin.join("file.txt"), "v2").unwrap();
    git(&sub_origin, &["add", "-A"]);
    git(&sub_origin, &["commit", "-qm", "c2"]);
    let second = git(&sub_origin, &["rev-parse", "HEAD"]).trim().to_string();

    let main = root.join("main");
    std::fs::create_dir(&main).unwrap();
    init_repo(&main);
    git(
        &main,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            "../sub-origin",
            "modules/sub",
        ],
    );
    git(&main.join("modules/sub"), &["checkout", "-q", &first]);
    git(&main, &["add", "-A"]);
    git(&main, &["commit", "-qm", "add submodule at c1"]);

    git(&main.join("modules/sub"), &["checkout", "-q", &second]);
    let dirty = git(&main, &["status", "--short"]);
    assert!(
        dirty.contains("modules/sub"),
        "precondition: submodule should show as modified, got: {dirty:?}"
    );

    GitCli::new(&main).discard_file("modules/sub").unwrap();

    let status = git(&main, &["status", "--short"]);
    assert!(
        status.trim().is_empty(),
        "discard should revert the submodule pointer, but status is still dirty: {status:?}"
    );
}
