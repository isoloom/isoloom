//! One check runner: where it stands, and the command that runs it.

use std::path::{Path, PathBuf};

use isoloom_spec::shell;

/// One check runner to execute: where it stands, and the command that runs it and prints the
/// `isoloom-check:` lines.
pub struct Runner {
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
    pub wd: PathBuf,
    pub env: Vec<(String, String)>,
}

/// The label of the runner `isoloom run`'s Kubernetes Jobs start with: it prints no check.
pub const STARTING_JOBS: &str = "starting the check Jobs";

impl Runner {
    pub fn new(label: impl Into<String>, program: &str, args: Vec<String>, wd: &Path) -> Self {
        Self {
            label: label.into(),
            program: program.to_string(),
            args,
            wd: wd.to_path_buf(),
            env: vec![],
        }
    }

    /// A runner that is a shell script.
    pub fn shell(label: impl Into<String>, script: String, wd: &Path) -> Self {
        Self::new(label, "sh", vec!["-c".to_string(), script], wd)
    }
}

/// A check runner reached over SSH (the cloud targets).
pub fn ssh_runner(key: Option<&Path>, user: &str, host: &str, command: &str, no_derived: bool) -> Runner {
    let mut args = vec![
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "BatchMode=yes".to_string(),
    ];
    if let Some(k) = key {
        args.extend(["-i".to_string(), k.display().to_string()]);
    }
    args.push(format!("{user}@{host}"));
    let derived = if no_derived { "ISOLOOM_DERIVED=0 " } else { "" };
    args.push(format!("{derived}{command}"));
    Runner {
        label: format!("on {host}"),
        program: "ssh".to_string(),
        args,
        wd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        env: vec![],
    }
}

/// An SSH runner with `script` on its stdin: `ssh ... < script` through a shell.
pub fn piped(mut r: Runner, script: &Path) -> Runner {
    let line = format!(
        "ssh {} < {}",
        r.args.iter().map(|a| shell::quote(a)).collect::<Vec<_>>().join(" "),
        shell::quote(&script.display().to_string())
    );
    r.program = "sh".to_string();
    r.args = vec!["-c".to_string(), line];
    r
}
