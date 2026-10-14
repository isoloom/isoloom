//! How commands print machine-readable output and turn a result into an exit code.

use std::process::ExitCode;

use crate::Res;

/// Prints a value as pretty JSON on stdout (the `--json` output of every command).
pub fn print_json(value: &impl serde::Serialize) -> Res<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Success when `ok`, else the generic failure (exit code 1).
pub fn exit_code(ok: bool) -> ExitCode {
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
