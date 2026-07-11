use git2::Oid;
use ravix::git::Repo;
use ravix::staging::build_patch;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap()
            .status
            .success(),
        "git {args:?}"
    );
}

#[test]
fn file_diff_keeps_the_no_newline_marker_intact() {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "d@e.com"]);
    git(dir.path(), &["config", "user.name", "Dev"]);
    std::fs::write(dir.path().join("f.txt"), "line one\nline two").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "no trailing newline"]);
    std::fs::write(dir.path().join("f.txt"), "line one\nline TWO").unwrap();

    let repo = Repo::discover(dir.path()).unwrap();
    let diff = repo.file_diff("f.txt", false).unwrap().unwrap();
    let lines: Vec<&str> = diff
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter().map(String::as_str))
        .collect();

    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("\\ No newline at end of file")),
        "the no-newline marker is emitted as git's literal marker, not prefixed: {lines:?}"
    );
}

#[test]
fn a_final_line_hunk_without_trailing_newline_applies_to_the_index() {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "d@e.com"]);
    git(dir.path(), &["config", "user.name", "Dev"]);
    std::fs::write(dir.path().join("f.txt"), "line one\nline two").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "no trailing newline"]);
    std::fs::write(dir.path().join("f.txt"), "line one\nline TWO").unwrap();

    let repo = Repo::discover(dir.path()).unwrap();
    let diff = repo.file_diff("f.txt", false).unwrap().unwrap();
    let patch = build_patch(&diff, &(0..diff.hunks.len()).collect::<Vec<_>>());
    std::fs::write(dir.path().join("hunk.patch"), &patch).unwrap();

    let output = Command::new("git")
        .current_dir(dir.path())
        .args(["apply", "--cached", "hunk.patch"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git apply rejected the hunk patch: {}\n--- patch ---\n{patch}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn commit_file_diff_shows_a_commit_against_its_parent() {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "d@e.com"]);
    git(dir.path(), &["config", "user.name", "Dev"]);
    std::fs::write(dir.path().join("f.txt"), "line one\nline two\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "first"]);
    std::fs::write(dir.path().join("f.txt"), "line one\nline TWO\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "second"]);

    let repo = Repo::discover(dir.path()).unwrap();
    let head = Oid::from_str(&repo.head_oid().unwrap()).unwrap();
    let diff = repo.commit_file_diff(head, "f.txt").unwrap().unwrap();
    let lines: Vec<&str> = diff
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter().map(String::as_str))
        .collect();

    assert!(lines.contains(&"-line two"), "the parent's line: {lines:?}");
    assert!(lines.contains(&"+line TWO"), "the commit's line: {lines:?}");
}

#[test]
fn commit_file_diff_of_the_root_commit_is_all_additions() {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "d@e.com"]);
    git(dir.path(), &["config", "user.name", "Dev"]);
    std::fs::write(dir.path().join("f.txt"), "only line\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "root"]);

    let repo = Repo::discover(dir.path()).unwrap();
    let root = Oid::from_str(&repo.head_oid().unwrap()).unwrap();
    let diff = repo.commit_file_diff(root, "f.txt").unwrap().unwrap();
    let lines: Vec<&str> = diff
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter().map(String::as_str))
        .collect();

    assert!(
        lines.contains(&"+only line"),
        "root diffs against empty: {lines:?}"
    );
    assert!(
        repo.commit_file_diff(root, "absent.txt").unwrap().is_none(),
        "a path not in the commit yields None"
    );
}
