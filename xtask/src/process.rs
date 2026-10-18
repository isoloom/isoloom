//! Running the tools the tasks drive: what to say when one fails, its output line by line, and
//! the program to start on Windows, where npm's tools are `.cmd` shims.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, ExitCode, ExitStatus, Stdio};
use std::sync::mpsc;

use crate::Res;

// The command describers, captured-output runners and the Windows program lookup are shared with
// Isothrum through the ratchet crate; the rest of the runners live here.
pub(crate) use xtask_quality::process::{describe, output, program, quiet, without_run_env};

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

/// Runs a command with its standard output and error read line by line, both handed to
/// `on_line` as they come (a carriage return at a line's end dropped), and nothing on its
/// standard input.
pub(crate) fn stream(cmd: &mut Command, on_line: &mut dyn FnMut(&str)) -> std::io::Result<ExitStatus> {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let (tx, rx) = mpsc::channel::<String>();
    let mut pipes: Vec<Box<dyn Read + Send>> = Vec::new();
    if let Some(out) = child.stdout.take() {
        pipes.push(Box::new(out));
    }
    if let Some(err) = child.stderr.take() {
        pipes.push(Box::new(err));
    }
    let readers: Vec<_> = pipes
        .into_iter()
        .map(|pipe| {
            let tx = tx.clone();
            std::thread::spawn(move || send_lines(pipe, &tx))
        })
        .collect();
    drop(tx);
    for line in rx {
        on_line(&line);
    }
    // A reader ends with its pipe; one that panicked has nothing more to hand over.
    for r in readers {
        r.join().ok();
    }
    child.wait()
}

fn send_lines(pipe: Box<dyn Read + Send>, tx: &mpsc::Sender<String>) {
    let mut reader = BufReader::new(pipe);
    let mut buf = Vec::new();
    while matches!(reader.read_until(b'\n', &mut buf), Ok(n) if n > 0) {
        let text = String::from_utf8_lossy(&buf);
        if tx.send(text.trim_end_matches(['\n', '\r']).to_owned()).is_err() {
            return;
        }
        buf.clear();
    }
}
