use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::askpass::AskpassConfig;
use crate::join::MergeTreeResult;
use crate::stash::{StashEntry, parse_stash_list};

#[derive(Debug)]
pub enum MutationError {
    Spawn(std::io::Error),
    Failed { command: String, stderr: String },
}

impl fmt::Display for MutationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(f, "could not run git: {error}"),
            Self::Failed { command, stderr } => write!(f, "{command}: {stderr}"),
        }
    }
}

pub struct GitCli {
    workdir: PathBuf,
    askpass: Option<AskpassConfig>,
}

impl GitCli {
    pub fn new(workdir: impl AsRef<Path>) -> Self {
        Self {
            workdir: workdir.as_ref().to_path_buf(),
            askpass: None,
        }
    }

    pub fn with_askpass(mut self, config: AskpassConfig) -> Self {
        self.askpass = Some(config);
        self
    }

    pub fn stage_file(&self, path: &str) -> Result<(), MutationError> {
        self.run(&["add", "--", path], None)
    }

    pub fn unstage_file(&self, path: &str) -> Result<(), MutationError> {
        self.run(&["restore", "--staged", "--", path], None)
    }

    pub fn stage_all(&self) -> Result<(), MutationError> {
        self.run(&["add", "-A"], None)
    }

    pub fn unstage_all(&self) -> Result<(), MutationError> {
        self.run(&["restore", "--staged", "."], None)
    }

    pub fn stage_hunk(&self, patch: &str) -> Result<(), MutationError> {
        self.run(&["apply", "--cached", "--whitespace=nowarn"], Some(patch))
    }

    pub fn unstage_hunk(&self, patch: &str) -> Result<(), MutationError> {
        self.run(
            &["apply", "--cached", "--reverse", "--whitespace=nowarn"],
            Some(patch),
        )
    }

    pub fn submodule_init(&self, path: &str) -> Result<(), MutationError> {
        self.run(&["submodule", "update", "--init", "--", path], None)
    }

    pub fn submodule_update(&self, path: &str) -> Result<(), MutationError> {
        self.run(&["submodule", "update", "--checkout", "--", path], None)
    }

    pub fn discard_file(&self, path: &str) -> Result<(), MutationError> {
        self.run(&["restore", "--recurse-submodules", "--", path], None)
    }

    pub fn discard_hunk(&self, patch: &str) -> Result<(), MutationError> {
        self.run(&["apply", "--reverse", "--whitespace=nowarn"], Some(patch))
    }

    pub fn remove_untracked(&self, path: &str) -> Result<(), MutationError> {
        std::fs::remove_file(self.workdir.join(path)).map_err(MutationError::Spawn)
    }

    pub fn commit(&self, message: &str) -> Result<(), MutationError> {
        self.run(&["commit", "--quiet", "-F", "-"], Some(message))
    }

    pub fn reset_soft_previous(&self) -> Result<(), MutationError> {
        self.run(&["reset", "--soft", "HEAD@{1}"], None)
    }

    pub fn reset_keep(&self, oid: &str) -> Result<(), MutationError> {
        self.run(&["reset", "--keep", oid], None)
    }

    pub fn switch_branch(&self, name: &str) -> Result<(), MutationError> {
        self.run(&["switch", name], None)
    }

    pub fn checkout_detached(&self, oid: &str) -> Result<(), MutationError> {
        self.run(&["checkout", oid], None)
    }

    pub fn checkout(&self, target: &str) -> Result<(), MutationError> {
        self.run(&["checkout", target], None)
    }

    pub fn create_branch(&self, name: &str, start: &str) -> Result<(), MutationError> {
        self.run(&["switch", "-c", name, start], None)
    }

    pub fn delete_branch(&self, name: &str) -> Result<(), MutationError> {
        self.run(&["branch", "-d", name], None)
    }

    pub fn force_delete_branch(&self, name: &str) -> Result<(), MutationError> {
        self.run(&["branch", "-D", name], None)
    }

    pub fn delete_remote_branch(&self, remote: &str, branch: &str) -> Result<(), MutationError> {
        self.run(&["push", remote, "--delete", branch], None)
    }

    pub fn switch_create_track(&self, name: &str, remote_ref: &str) -> Result<(), MutationError> {
        self.run(&["switch", "-c", name, "--track", remote_ref], None)
    }

    pub fn create_branch_at(&self, name: &str, oid: &str) -> Result<(), MutationError> {
        self.run(&["branch", name, oid], None)
    }

    pub fn merge_ff(&self, source: &str) -> Result<(), MutationError> {
        self.run(&["merge", "--ff-only", source], None)
    }

    pub fn merge_no_ff(&self, source: &str) -> Result<(), MutationError> {
        self.run(&["merge", "--no-ff", "--no-edit", source], None)
    }

    pub fn cherry_pick(&self, oid: &str) -> Result<(), MutationError> {
        self.run(&["cherry-pick", oid], None)
    }

    pub fn cherry_pick_abort(&self) -> Result<(), MutationError> {
        self.run(&["cherry-pick", "--abort"], None)
    }

    pub fn merge_abort(&self) -> Result<(), MutationError> {
        self.run(&["merge", "--abort"], None)
    }

    pub fn fetch(&self) -> Result<(), MutationError> {
        self.run(&["fetch"], None)
    }

    pub fn push(&self) -> Result<(), MutationError> {
        self.run(&["push"], None)
    }

    pub fn push_set_upstream(&self, remote: &str, branch: &str) -> Result<(), MutationError> {
        self.run(&["push", "-u", remote, branch], None)
    }

    pub fn push_force_with_lease(&self) -> Result<(), MutationError> {
        self.run(&["push", "--force-with-lease"], None)
    }

    pub fn rebase(&self, target: &str) -> Result<(), MutationError> {
        self.run(&["-c", "core.editor=true", "rebase", target], None)
    }

    pub fn rebase_continue(&self) -> Result<(), MutationError> {
        self.run(&["-c", "core.editor=true", "rebase", "--continue"], None)
    }

    pub fn rebase_skip(&self) -> Result<(), MutationError> {
        self.run(&["rebase", "--skip"], None)
    }

    pub fn rebase_abort(&self) -> Result<(), MutationError> {
        self.run(&["rebase", "--abort"], None)
    }

    pub fn stash_save(&self) -> Result<(), MutationError> {
        self.run(&["stash", "push", "--include-untracked"], None)
    }

    pub fn stash_pop(&self, index: usize) -> Result<(), MutationError> {
        self.run(&["stash", "pop", &format!("stash@{{{index}}}")], None)
    }

    pub fn stash_apply(&self, index: usize) -> Result<(), MutationError> {
        self.run(&["stash", "apply", &format!("stash@{{{index}}}")], None)
    }

    pub fn stash_drop(&self, index: usize) -> Result<(), MutationError> {
        self.run(&["stash", "drop", &format!("stash@{{{index}}}")], None)
    }

    pub fn stash_list(&self) -> Vec<StashEntry> {
        let output = Command::new("git")
            .current_dir(&self.workdir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .args(["stash", "list"])
            .output();
        match output {
            Ok(output) => parse_stash_list(&String::from_utf8_lossy(&output.stdout)),
            Err(_) => Vec::new(),
        }
    }

    pub fn merge_tree(&self, ours: &str, theirs: &str, base: Option<&str>) -> MergeTreeResult {
        let mut args: Vec<String> = vec![
            "merge-tree".into(),
            "--write-tree".into(),
            "--name-only".into(),
            "--no-messages".into(),
        ];
        if let Some(base) = base {
            args.push(format!("--merge-base={base}"));
        }
        args.push(ours.into());
        args.push(theirs.into());

        match Command::new("git")
            .current_dir(&self.workdir)
            .args(&args)
            .output()
        {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                let files = text
                    .lines()
                    .skip(1)
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect();
                MergeTreeResult {
                    conflicted: !output.status.success(),
                    files,
                }
            }
            Err(_) => MergeTreeResult {
                conflicted: true,
                files: Vec::new(),
            },
        }
    }

    fn run(&self, args: &[&str], stdin: Option<&str>) -> Result<(), MutationError> {
        let mut command = Command::new("git");
        command
            .current_dir(&self.workdir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .args(args);
        if let Some(config) = &self.askpass {
            command
                .env("GIT_ASKPASS", &config.helper)
                .env(crate::askpass::SOCK_ENV, &config.socket);
        }
        command
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(MutationError::Spawn)?;
        if let Some(input) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .map_err(MutationError::Spawn)?;
        }

        let output = child.wait_with_output().map_err(MutationError::Spawn)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(MutationError::Failed {
                command: format!("git {}", args.join(" ")),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            })
        }
    }
}
