mod common;

use tempfile::TempDir;

use common::{git, init_repo};
use ravix::mutate::GitCli;

#[test]
fn a_conflicting_apply_reports_what_git_printed_on_stdout() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("f.txt"), "a\nb\nc\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(dir.path().join("f.txt"), "a\nSTASHED\nc\n").unwrap();
    git(dir.path(), &["stash", "push", "-q", "-m", "wip"]);
    std::fs::write(dir.path().join("f.txt"), "a\nHEAD\nc\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-qm", "diverge"]);

    let cli = GitCli::new(dir.path());
    let error = cli.stash_apply(0).unwrap_err();
    let message = error.to_string();

    assert!(
        message.contains("CONFLICT"),
        "git writes the conflict on stdout, it must survive: {message}"
    );
    assert_eq!(message.lines().count(), 1, "the status bar is one line");
}

#[test]
fn a_stderr_failure_still_reports_its_reason() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("f.txt"), "a\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-qm", "base"]);

    let cli = GitCli::new(dir.path());
    let message = cli.switch_branch("nope").unwrap_err().to_string();

    assert!(
        message.contains("fatal:"),
        "a stderr failure should keep its reason: {message}"
    );
}
