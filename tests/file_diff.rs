use ogma::git::Repo;
use ogma::staging::build_patch;
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
