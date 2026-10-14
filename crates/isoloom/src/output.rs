//! How commands print machine-readable output and turn a result into an exit code.
//!
//! A lifecycle command run with `--json` prints events, one JSON object per line on stdout
//! (`isoloom_core::events`), and, when it fails, the error document as its last line. Its
//! human-readable lines are left out; the tools' own output comes as `log` events. Without
//! `--json` nothing here prints.

use std::process::ExitCode;
use std::sync::{Mutex, OnceLock};

use isoloom_core::events::{Emitter, ErrorDocument, Scope, State, write_line};

use crate::Res;
use crate::failure;

/// The events' destination, set once when a command asks for JSON.
static EVENTS: OnceLock<Emitter> = OnceLock::new();

/// The step running now, named by the error document when the command fails during it.
static CURRENT: Mutex<Option<Scope>> = Mutex::new(None);

/// Prints a value as pretty JSON on stdout (the `--json` output of every command).
pub fn print_json(value: &impl serde::Serialize) -> Res<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Success when `ok`, else the generic failure (exit code 1).
pub fn exit_code(ok: bool) -> ExitCode {
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Turns JSON output on for this command, when `on` (its `--json`).
pub fn use_json(on: bool) {
    if on {
        let _ = EVENTS.set(Emitter::json_lines(std::io::stdout()));
    }
}

/// Whether the command prints JSON.
pub fn json() -> bool {
    EVENTS.get().is_some()
}

/// Where events go: JSON lines on stdout with `--json`, nowhere otherwise.
pub fn events() -> Emitter {
    EVENTS.get().cloned().unwrap_or_default()
}

/// A step starts.
pub fn begin(at: &Scope, detail: Option<String>) {
    events().step(at, State::Running, detail);
    if let Ok(mut current) = CURRENT.lock() {
        *current = Some(at.clone());
    }
}

/// A step starts: an event with `--json`, else `line` on the terminal (stderr), as the
/// command printed it before events existed.
pub fn announce(at: &Scope, detail: Option<String>, line: &str) {
    if json() {
        begin(at, detail);
    } else {
        eprintln!("{line}");
    }
}

/// The step that started last ends well.
pub fn done(at: &Scope, detail: Option<String>) {
    end(at, State::Done, detail);
}

/// The step that started last failed, and the command goes on (or reports it in its result):
/// an event with `--json`, else `line` on the terminal (stderr) when there is one.
pub fn failed(at: &Scope, why: &str, line: Option<&str>) {
    if json() {
        end(at, State::Failed, Some(why.to_string()));
    } else if let Some(l) = line {
        eprintln!("{l}");
    }
}

fn end(at: &Scope, state: State, detail: Option<String>) {
    events().step(at, state, detail);
    if let Ok(mut current) = CURRENT.lock() {
        *current = None;
    }
}

/// The command's outcome, its last event.
pub fn result(command: &str, ok: bool, data: serde_json::Value) {
    events().result(command, ok, data);
}

/// With `--json`, a failed command's last lines: the running step marked failed, then the
/// error document. False when the command doesn't print JSON (the caller prints the error).
pub fn print_error(e: &(dyn std::error::Error + 'static)) -> bool {
    if !json() {
        return false;
    }
    let mut body = failure::body_of(e);
    let running = CURRENT.lock().ok().and_then(|mut c| c.take());
    if let Some(at) = running {
        events().step(&at, State::Failed, Some(body.message.clone()));
        body.step = body.step.or(Some(at.step));
        body.machine = body.machine.or(at.machine);
    }
    let _ = write_line(&mut std::io::stdout(), &ErrorDocument { error: body });
    true
}
