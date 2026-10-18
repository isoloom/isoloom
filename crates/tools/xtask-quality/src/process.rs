//! Running the tools the ratchet drives: what to say when one fails, its captured output, and
//! the program to start on Windows, where npm's tools are `.cmd` shims. The subset the quality
//! checks use; the full set lives in each repository's own `xtask`.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Res;

/// The command as one line, for messages and logs.
#[must_use]
pub fn describe(cmd: &Command) -> String {
    let mut parts = vec![cmd.get_program().to_string_lossy().into_owned()];
    parts.extend(cmd.get_args().map(|a| a.to_string_lossy().into_owned()));
    parts.join(" ")
}

/// The variables `cargo run` sets for the xtask itself (`CARGO_PKG_NAME`, `CARGO_MANIFEST_DIR`
/// and the like) taken out of `cmd`'s environment: they describe the xtask, not what it runs,
/// and some tools read them (cargo-machete takes `CARGO_PKG_NAME` as a sign it runs inside a
/// build script, and reads its arguments wrong).
pub fn without_run_env(cmd: &mut Command) -> &mut Command {
    const SET_BY_RUN: [&str; 4] = ["CARGO_CRATE_NAME", "CARGO_BIN_NAME", "CARGO_PRIMARY_PACKAGE", "CARGO_RUSTC_CURRENT_DIR"];
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if name.starts_with("CARGO_PKG_") || name.starts_with("CARGO_MANIFEST_") || SET_BY_RUN.contains(&name.as_ref()) {
            cmd.env_remove(&key);
        }
    }
    cmd
}

/// A command's standard output, when it succeeds.
///
/// # Errors
///
/// Fails when the command can't start or exits unsuccessfully.
pub fn output(cmd: &mut Command) -> Res<String> {
    let out = cmd
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("`{}` couldn't start: {e}", describe(cmd)))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!("`{}` failed ({})", describe(cmd), out.status).into())
    }
}

/// Runs a command with its output captured: nothing on the terminal, and the end of what it
/// said in the error when it fails.
///
/// # Errors
///
/// Fails when the command can't start or exits unsuccessfully (the error carries the tail of its
/// standard error).
pub fn quiet(cmd: &mut Command) -> Res<()> {
    let out = cmd.output().map_err(|e| format!("`{}` couldn't start: {e}", describe(cmd)))?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    let tail: Vec<&str> = said.lines().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect();
    Err(format!("`{}` failed ({}): {}", describe(cmd), out.status, tail.join(" / ")).into())
}

/// The program to start for `name`: on Windows, the `.exe`/`.cmd`/`.bat` on `path` (npm's tools
/// are `.cmd` shims there); elsewhere, `name` itself.
#[must_use]
pub fn program(name: &str, path: &OsStr) -> OsString {
    if !cfg!(windows) || Path::new(name).extension().is_some() {
        return name.into();
    }
    let found = std::env::split_paths(path).find_map(|dir| ["exe", "cmd", "bat"].iter().map(|ext| dir.join(format!("{name}.{ext}"))).find(|p| p.is_file()));
    found.map_or_else(|| name.into(), PathBuf::into_os_string)
}
