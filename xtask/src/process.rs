//! Running the tools the tasks drive: the cargo running this xtask, a command attached to the
//! terminal, and what to say when one fails.

use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

use crate::Res;

// The command describers and captured-output runners are shared with Isothrum through the ratchet
// crate; the rest of the runners live here.
pub(crate) use xtask_quality::process::{describe, output, quiet, without_run_env};

/// `cargo`, the one running this xtask (`$CARGO`, so the same toolchain), in `dir`, without the
/// variables `cargo run` set for the xtask.
pub(crate) fn cargo(dir: &Path) -> Command {
    let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    without_run_env(&mut cmd);
    cmd.current_dir(dir);
    cmd
}

/// Runs a command attached to the terminal; whether it succeeded. An error only when it can't
/// start.
pub(crate) fn passed(cmd: &mut Command) -> Res<bool> {
    let status = cmd.status().map_err(|e| format!("`{}` couldn't start: {e}", describe(cmd)))?;
    Ok(status.success())
}

/// The exit code of a task whose steps all had to pass.
pub(crate) fn exit_code(ok: bool) -> ExitCode {
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Runs a command attached to the terminal; an error names it when it fails.
pub(crate) fn run(cmd: &mut Command) -> Res<()> {
    let status = cmd.status().map_err(|e| format!("`{}` couldn't start: {e}", describe(cmd)))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{}` failed ({status})", describe(cmd)).into())
    }
}

/// The first line a command prints on success, or `None` (not installed, or it failed).
pub(crate) fn first_line(cmd: &mut Command) -> Option<String> {
    let out = cmd.stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    out.status.success().then(|| text.lines().next().unwrap_or("").trim().to_owned())
}
