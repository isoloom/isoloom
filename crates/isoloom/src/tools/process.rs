//! Running a tool: every one started with the profile's inputs in its environment, its output
//! on the terminal or (with `--json`) as `log` events, and what to say when it can't start.

use std::ffi::OsStr;
use std::process::{Command, ExitCode, ExitStatus};

use isoloom_runtime::events::{ErrorCode, Scope};

use crate::failure::Failure;
use crate::{Res, output, spec};

/// A command for `program`, with the profile's inputs in its environment (see
/// [`spec::child_env`]). Every tool the command line starts is made here.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut c = Command::new(program);
    c.envs(spec::child_env().iter().map(|(k, v)| (k, v)));
    c
}

/// Whether a terminal is attached (so `-t` can be asked of the tools).
pub fn terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// Why a command couldn't start: the tool isn't installed, or the system's reason.
pub fn tool_error(c: &Command, e: &std::io::Error) -> Failure {
    let program = c.get_program().to_string_lossy();
    if e.kind() == std::io::ErrorKind::NotFound {
        spawn_error(&program, e)
    } else {
        Failure::new(ErrorCode::Io, format!("{program}: {e}"))
    }
}

/// Why `program` couldn't start: it isn't installed, or the system's reason alone.
pub fn spawn_error(program: &str, e: &std::io::Error) -> Failure {
    if e.kind() == std::io::ErrorKind::NotFound {
        Failure::new(ErrorCode::ToolMissing, format!("`{program}` isn't installed"))
    } else {
        Failure::new(ErrorCode::Io, e.to_string())
    }
}

/// A private per-run directory under the system's temporary folder, `<prefix>-<pid>-<nonce>`,
/// created exclusively (fails if it already exists), so a local attacker can't pre-place a
/// symlink at a guessable path and have us clobber a victim file or run a swapped script.
pub fn private_dir(prefix: &str) -> Res<std::path::PathBuf> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nonce:x}", std::process::id()));
    std::fs::create_dir(&dir)?;
    Ok(dir)
}

/// Runs a command attached to the terminal; its success is the exit code.
pub fn run_attached(c: &mut Command) -> Res<ExitCode> {
    let status = c.status().map_err(|e| tool_error(c, &e))?;
    Ok(output::exit_code(status.success()))
}

/// Runs a tool for a step to its end: on the terminal, or with `--json` its output as `log`
/// events of the step (so nothing but events reaches stdout).
pub fn run_step(c: &mut Command, at: &Scope) -> Res<ExitStatus> {
    run_step_input(c, None, at)
}

/// [`run_step`] with `input` on the tool's standard input when given.
pub fn run_step_input(c: &mut Command, input: Option<&[u8]>, at: &Scope) -> Res<ExitStatus> {
    let status = if output::json() {
        isoloom_runtime::tool::run_logged_input(c, input, &output::events(), at)
    } else if input.is_some() {
        c.stdin(std::process::Stdio::piped());
        c.spawn().and_then(|mut child| {
            let writer = isoloom_runtime::tool::feed(&mut child, input);
            let status = child.wait();
            if let Some(w) = writer {
                let _ = w.join();
            }
            status
        })
    } else {
        c.status()
    };
    Ok(status.map_err(|e| tool_error(c, &e))?)
}
