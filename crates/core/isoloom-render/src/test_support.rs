//! What tests of the crates above this one build their cases from, behind the `test-support`
//! feature (a dev-dependency's): never part of the API a backend renders with.

use crate::run_manifest::guest::{GuestCommand, GuestCompose};

/// A guest with nothing in it but `status`: no machines, slots, steps or checks.
#[must_use]
pub fn empty_guest(status: GuestCommand) -> GuestCompose {
    GuestCompose {
        machines: Vec::new(),
        networks: Vec::new(),
        slots: Vec::new(),
        params: Vec::new(),
        start: Vec::new(),
        ready: Vec::new(),
        check_setup: Vec::new(),
        check_teardown: Vec::new(),
        runners: Vec::new(),
        status,
    }
}

/// A POSIX `sh` to run a generated script with: the system's on Unix; on Windows the one on
/// `PATH` (Git's `bin` has one), else `None`, and the test that asked passes without running it,
/// as those needing Ruby or `tar` do. The scripts are for Linux guests: a Windows computer runs
/// them only when it has a `sh` to.
#[must_use]
pub fn posix_sh() -> Option<std::process::Command> {
    #[cfg(windows)]
    if std::process::Command::new("sh").args(["-c", "true"]).output().is_err() {
        return None;
    }
    Some(std::process::Command::new("sh"))
}

/// `path` as a script for `sh` takes it: with forward slashes, which Git's `sh` on Windows reads
/// as the folders they separate (it reads a backslash as an escape).
#[must_use]
pub fn sh_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
