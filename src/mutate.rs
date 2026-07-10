use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
}

impl GitCli {
    pub fn new(workdir: impl AsRef<Path>) -> Self {
        Self {
            workdir: workdir.as_ref().to_path_buf(),
        }
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

    pub fn discard_file(&self, path: &str) -> Result<(), MutationError> {
        self.run(&["restore", "--", path], None)
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

    fn run(&self, args: &[&str], stdin: Option<&str>) -> Result<(), MutationError> {
        let mut command = Command::new("git");
        command
            .current_dir(&self.workdir)
            .args(args)
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
