//! Running a tool: whether a terminal is attached, and what to say when it can't start.

use std::process::{Command, ExitCode};

use crate::{Res, output};

/// Whether a terminal is attached (so `-t` can be asked of the tools).
pub fn terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// Why a command couldn't start: the tool isn't installed, or the system's reason.
pub fn tool_error(c: &Command, e: std::io::Error) -> String {
    let program = c.get_program().to_string_lossy();
    if e.kind() == std::io::ErrorKind::NotFound {
        format!("`{program}` isn't installed")
    } else {
        format!("{program}: {e}")
    }
}

/// Why `program` couldn't start: it isn't installed, or the system's reason alone.
pub fn spawn_error(program: &str, e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        format!("`{program}` isn't installed")
    } else {
        e.to_string()
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
    let status = c.status().map_err(|e| tool_error(c, e))?;
    Ok(output::exit_code(status.success()))
}
