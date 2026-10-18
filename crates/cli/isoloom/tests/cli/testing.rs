//! What the command line's tests share: a fresh folder per test, a project holding a spec, the
//! repository's examples, and the binary run with its own `ISOLOOM_HOME`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A test's outcome: a failure is the report.
pub(crate) type Outcome<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// A fresh, empty folder under the system's temporary folder, unique to this test.
pub(crate) fn scratch(name: &str) -> Outcome<PathBuf> {
    let dir = std::env::temp_dir().join(format!("isoloom-cli-{}-{name}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// A fresh project folder holding `yaml` as its isoloom.yml.
pub(crate) fn project(name: &str, yaml: &str) -> Outcome<PathBuf> {
    let dir = scratch(name)?;
    std::fs::write(dir.join("isoloom.yml"), yaml)?;
    Ok(dir)
}

/// The repository's example `name`.
pub(crate) fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name)
}

/// What a run of the binary gave: its exit code (none when a signal ended it) and its output.
#[derive(Debug)]
pub(crate) struct Run {
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// `isoloom <args>` with `home` as its state folder and `env` set.
pub(crate) fn isoloom_with(home: &Path, args: &[&dyn AsRef<std::ffi::OsStr>], env: &[(&str, &str)]) -> Outcome<Run> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_isoloom"));
    for arg in args {
        cmd.arg(arg);
    }
    let out = cmd.env("ISOLOOM_HOME", home).env_remove("ISOLOOM_PROFILE").envs(env.iter().copied()).output()?;
    Ok(Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// `isoloom <args>` with `home` as its state folder.
pub(crate) fn isoloom(home: &Path, args: &[&dyn AsRef<std::ffi::OsStr>]) -> Outcome<Run> {
    isoloom_with(home, args, &[])
}

/// `isoloom validate <target> --json`, with the run and its parsed report.
pub(crate) fn validate_json(home: &Path, target: &dyn AsRef<std::ffi::OsStr>) -> Outcome<(Run, serde_json::Value)> {
    let run = isoloom(home, &[&"validate", target, &"--json"])?;
    let report = serde_json::from_str(&run.stdout)?;
    Ok((run, report))
}
