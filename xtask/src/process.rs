//! Running the tools the tasks drive: what to say when one fails, its output line by line, and
//! the program to start on Windows, where npm's tools are `.cmd` shims.

use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;

use crate::Res;

/// The command as one line, for messages and logs.
pub fn describe(cmd: &Command) -> String {
    let mut parts = vec![cmd.get_program().to_string_lossy().into_owned()];
    parts.extend(cmd.get_args().map(|a| a.to_string_lossy().into_owned()));
    parts.join(" ")
}

/// The variables `cargo run` sets for the xtask itself (`CARGO_PKG_NAME`, `CARGO_MANIFEST_DIR`
/// and the like) taken out of `cmd`'s environment: they describe the xtask, not what it runs,
/// and some tools read them (cargo-machete takes `CARGO_PKG_NAME` as a sign it runs inside a
/// build script, and reads its arguments wrong).
pub(crate) fn without_run_env(cmd: &mut Command) -> &mut Command {
    const SET_BY_RUN: [&str; 4] = ["CARGO_CRATE_NAME", "CARGO_BIN_NAME", "CARGO_PRIMARY_PACKAGE", "CARGO_RUSTC_CURRENT_DIR"];
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if name.starts_with("CARGO_PKG_") || name.starts_with("CARGO_MANIFEST_") || SET_BY_RUN.contains(&name.as_ref()) {
            cmd.env_remove(&key);
        }
    }
    cmd
}

/// Runs a command attached to the terminal; an error names it when it fails.
pub fn run(cmd: &mut Command) -> Res<()> {
    let status = cmd.status().map_err(|e| format!("`{}` couldn't start: {e}", describe(cmd)))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{}` failed ({status})", describe(cmd)).into())
    }
}

/// A command's standard output, when it succeeds.
pub fn output(cmd: &mut Command) -> Res<String> {
    let out = cmd
        .stderr(Stdio::inherit())
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
pub fn quiet(cmd: &mut Command) -> Res<()> {
    let out = cmd.output().map_err(|e| format!("`{}` couldn't start: {e}", describe(cmd)))?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    let tail: Vec<&str> = said.lines().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect();
    Err(format!("`{}` failed ({}): {}", describe(cmd), out.status, tail.join(" / ")).into())
}

/// The first line a command prints on success, or `None` (not installed, or it failed).
pub fn first_line(cmd: &mut Command) -> Option<String> {
    let out = cmd.stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    out.status.success().then(|| text.lines().next().unwrap_or("").trim().to_string())
}

/// Runs a command with its standard output and error read line by line, both handed to
/// `on_line` as they come (a carriage return at a line's end dropped), and nothing on its
/// standard input.
pub fn stream(cmd: &mut Command, on_line: &mut dyn FnMut(&str)) -> std::io::Result<ExitStatus> {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let (tx, rx) = mpsc::channel::<String>();
    let readers: Vec<_> = [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|pipe| {
        let tx = tx.clone();
        std::thread::spawn(move || send_lines(pipe, &tx))
    })
    .collect();
    drop(tx);
    for line in rx {
        on_line(&line);
    }
    for r in readers {
        let _ = r.join();
    }
    child.wait()
}

fn send_lines(pipe: Box<dyn Read + Send>, tx: &mpsc::Sender<String>) {
    let mut reader = BufReader::new(pipe);
    let mut buf = Vec::new();
    while matches!(reader.read_until(b'\n', &mut buf), Ok(n) if n > 0) {
        let text = String::from_utf8_lossy(&buf);
        if tx.send(text.trim_end_matches(['\n', '\r']).to_string()).is_err() {
            return;
        }
        buf.clear();
    }
}

/// The program to start for `name` with `path` as the search path. On Windows a tool npm or
/// corepack installed is a `.cmd` shim, which `Command` doesn't look for: its full path is
/// returned. Elsewhere, and for a name not found, `name` itself.
pub fn program(name: &str, path: &OsStr) -> OsString {
    if !cfg!(windows) || Path::new(name).extension().is_some() {
        return name.into();
    }
    let found = std::env::split_paths(path).find_map(|dir| ["exe", "cmd", "bat"].iter().map(|ext| dir.join(format!("{name}.{ext}"))).find(|p| p.is_file()));
    found.map_or_else(|| name.into(), PathBuf::into_os_string)
}
