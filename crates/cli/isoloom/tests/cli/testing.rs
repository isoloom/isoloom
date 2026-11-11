//! What the command line's tests share: a fresh folder per test, a project holding a spec, the
//! repository's examples, and the binary run with its own `ISOLOOM_HOME`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A test's outcome: a failure is the report.
pub(crate) type Outcome<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// A test's own folder under the system's temporary folder, removed when it goes out of scope,
/// by a failed assertion's panic too: a run leaves nothing behind. It reads as the folder's path.
pub(crate) struct Scratch(tempfile::TempDir);

impl std::ops::Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        self.0.path()
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        self.0.path()
    }
}

impl AsRef<std::ffi::OsStr> for Scratch {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.path().as_os_str()
    }
}

/// A fresh, empty folder for test `name`, named after it.
pub(crate) fn scratch(name: &str) -> Outcome<Scratch> {
    Ok(Scratch(tempfile::Builder::new().prefix(&format!("isoloom-cli-{name}-")).tempdir()?))
}

/// A fresh project folder holding `yaml` as its isoloom.yml.
pub(crate) fn project(name: &str, yaml: &str) -> Outcome<Scratch> {
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

#[test]
fn a_scratch_folder_is_gone_once_its_test_ends_by_a_panic_too() -> Outcome {
    let dir = scratch("gone")?;
    std::fs::write(dir.join("a-file"), "")?;
    let path = dir.to_path_buf();
    drop(dir);
    assert!(!path.exists(), "{} is left", path.display());
    let seen = std::sync::Mutex::new(None);
    let failed = std::panic::catch_unwind(|| {
        let panicked = scratch("panicked").unwrap_or_else(|e| panic!("{e}"));
        *seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(panicked.to_path_buf());
        panic!("a failed assertion");
    });
    assert!(failed.is_err());
    let made = seen.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner).ok_or("no folder made")?;
    assert!(!made.exists(), "{} is left", made.display());
    Ok(())
}
