//! The machine-readable output of the lifecycle: what a command is doing, as events, and why it
//! failed, as one error document with a stable code.
//!
//! The command line's `--json` prints them one per line (newline-delimited JSON). A tool
//! embedding Isoloom gets the same events from an [`Emitter`]: a callback ([`Emitter::new`]) or
//! a channel ([`Emitter::channel`]). Their JSON Schema is [`schema`] (`isoloom schema --events`,
//! and `docs/events.schema.json`); `docs/json.md` describes them.
//!
//! The shapes are a contract: a field is never renamed or removed, and an error code never
//! changes meaning. New event kinds, fields and codes can appear: ignore what you don't know.

use std::io::Write;
use std::sync::{Arc, Mutex, mpsc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where the events and errors' schema is published.
pub const EVENTS_SCHEMA_URL: &str = "https://www.isoloom.com/schema/events-v1.json";

/// Where a step stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Started, not finished yet.
    Running,
    /// Finished and succeeded.
    Done,
    /// Finished and failed; an error document usually follows.
    Failed,
}

/// Which output of the underlying tool a log line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    /// Its standard output.
    Stdout,
    /// Its standard error (where most tools print their progress).
    Stderr,
}

/// One thing a lifecycle command reports while it runs. Every event carries the time it was
/// emitted (`YYYY-MM-DDTHH:MM:SSZ`, UTC).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A step of the command started (`running`), finished (`done`) or failed (`failed`).
    Step {
        /// The target the environment runs on (`docker`, `vagrant`...), when known.
        target: Option<String>,
        /// The machine the step is about; null for the whole environment.
        machine: Option<String>,
        /// The step: see the list in `docs/json.md` (`prepare`, `up`, `checks`...).
        step: String,
        /// Where it stands.
        state: State,
        /// A short human-readable detail (a folder, a count, a reason).
        detail: Option<String>,
        /// When.
        time: String,
    },
    /// A line the underlying tool (docker compose, vagrant, terraform, kubectl, ssh) printed
    /// during a step.
    Log {
        /// The target, when known.
        target: Option<String>,
        /// The machine the tool works on, when it is one.
        machine: Option<String>,
        /// The step the tool runs for.
        step: String,
        /// Which of the tool's outputs.
        stream: Stream,
        /// The line, without its newline.
        line: String,
        /// When.
        time: String,
    },
    /// One check's result (`isoloom test`).
    Check {
        /// The target.
        target: Option<String>,
        /// Where the check ran from: a machine, or the environment's networks.
        position: String,
        /// The check's name.
        name: String,
        /// Whether it passed.
        ok: bool,
        /// Why it failed (empty when it passed).
        detail: String,
        /// When.
        time: String,
    },
    /// The command's outcome: the last event of a command that succeeded.
    Result {
        /// The subcommand (`run`, `test`...).
        command: String,
        /// Whether it reached its goal (false: checks failed, a machine's command failed).
        ok: bool,
        /// What the command found or produced: see `docs/json.md` for each command.
        data: serde_json::Value,
        /// When.
        time: String,
    },
    /// An event of a kind this version doesn't know (a newer Isoloom's), read rather than
    /// refused. Never emitted.
    #[serde(other)]
    #[schemars(skip)]
    Unknown,
}

/// The stable codes of errors: what kind of failure, for a program to react to without reading
/// the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorCode {
    /// The command line's arguments don't make sense together (an unknown action, a missing
    /// command after `--`).
    Usage,
    /// No spec in the folder, or it can't be read or parsed.
    SpecLoad,
    /// The spec parses but has problems (listed in `problems`).
    SpecInvalid,
    /// The profile asked for can't be read or doesn't fit the spec.
    Profile,
    /// A launch-time input the environment needs isn't set (in the shell or the profile's
    /// `inputs`).
    InputMissing,
    /// No target by that name.
    TargetUnknown,
    /// The spec can't run on the target, or several targets are possible and none was named.
    TargetUnavailable,
    /// The target's files couldn't be generated.
    GenerateFailed,
    /// This host lacks what the target needs (a tool, a credential): see `isoloom doctor`.
    HostNotReady,
    /// A tool the target runs with isn't installed.
    ToolMissing,
    /// The tool ran and failed (its output came as `log` events).
    ToolFailed,
    /// Nothing is running there: run the environment first.
    NotRunning,
    /// No machine (or tool, or group) by that name in the environment.
    MachineUnknown,
    /// No network by that name, or the machine isn't on it.
    NetworkUnknown,
    /// The command doesn't do that on this target yet.
    Unsupported,
    /// Reading or writing a file failed.
    Io,
    /// Anything else.
    Internal,
    /// A code this version doesn't know (a newer Isoloom's), read rather than refused: treat it
    /// as `internal`. Never emitted.
    #[serde(other)]
    #[schemars(skip)]
    Unknown,
}

impl ErrorCode {
    /// The code as written in JSON (`spec_invalid`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::SpecLoad => "spec_load",
            Self::SpecInvalid => "spec_invalid",
            Self::Profile => "profile",
            Self::InputMissing => "input_missing",
            Self::TargetUnknown => "target_unknown",
            Self::TargetUnavailable => "target_unavailable",
            Self::GenerateFailed => "generate_failed",
            Self::HostNotReady => "host_not_ready",
            Self::ToolMissing => "tool_missing",
            Self::ToolFailed => "tool_failed",
            Self::NotRunning => "not_running",
            Self::MachineUnknown => "machine_unknown",
            Self::NetworkUnknown => "network_unknown",
            Self::Unsupported => "unsupported",
            Self::Io => "io",
            Self::Internal => "internal",
            Self::Unknown => "unknown",
        }
    }
}

/// One problem of an invalid spec: the field's path and the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FieldProblem {
    /// The field, e.g. `machines.web.depends_on[0]`.
    pub at: String,
    /// What is wrong, and the fix when there is one.
    pub message: String,
}

/// Why a command failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorBody {
    /// What kind of failure: stable, see [`ErrorCode`].
    pub code: ErrorCode,
    /// The message the command line prints after `error:`.
    pub message: String,
    /// The machine it is about, when it is one.
    pub machine: Option<String>,
    /// The step that failed, when one was running (see the `step` events).
    pub step: Option<String>,
    /// What to do about it, when there is something to say beyond the message.
    pub hint: Option<String>,
    /// The spec's problems, for `spec_invalid` (empty otherwise).
    pub problems: Vec<FieldProblem>,
}

impl ErrorBody {
    /// An error with a code and a message, nothing else known.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            machine: None,
            step: None,
            hint: None,
            problems: Vec::new(),
        }
    }
}

/// The last line a failed command prints with `--json`: `{ "error": { ... } }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorDocument {
    /// Why the command failed.
    pub error: ErrorBody,
}

/// One line of `--json` output: an event, or the error that ends the command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Line {
    /// Something the command did.
    Event(Event),
    /// Why it failed.
    Error(ErrorDocument),
}

/// What an event is about: the target, the machine (when one) and the step.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    /// The target's id.
    pub target: Option<String>,
    /// The machine, when the step is about one.
    pub machine: Option<String>,
    /// The step's name.
    pub step: String,
}

impl Scope {
    /// A step of the whole environment on `target`.
    pub fn step(target: Option<&str>, step: &str) -> Self {
        Self {
            target: target.map(str::to_string),
            machine: None,
            step: step.to_string(),
        }
    }

    /// The same step, about `machine`.
    #[must_use]
    pub fn on(mut self, machine: &str) -> Self {
        self.machine = Some(machine.to_string());
        self
    }
}

/// A callback receiving events.
type Sink = Arc<dyn Fn(&Event) + Send + Sync>;

/// Where the lifecycle's events go: nowhere, a callback, a channel, or JSON lines on a writer.
/// Cheap to clone; every clone sends to the same place.
#[derive(Clone, Default)]
pub struct Emitter {
    sink: Option<Sink>,
}

impl std::fmt::Debug for Emitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Emitter").field("silent", &self.sink.is_none()).finish()
    }
}

impl Emitter {
    /// Events go to `f`, called on the thread that emits them (the tools' output is read on
    /// threads of its own).
    pub fn new(f: impl Fn(&Event) + Send + Sync + 'static) -> Self {
        Self { sink: Some(Arc::new(f)) }
    }

    /// Events go nowhere.
    pub fn silent() -> Self {
        Self::default()
    }

    /// Events go to a channel; the receiver ends when every clone of the emitter is dropped.
    pub fn channel() -> (Self, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let emitter = Self::new(move |e| {
            if let Ok(tx) = tx.lock() {
                let _ = tx.send(e.clone());
            }
        });
        (emitter, rx)
    }

    /// Events written to `w` as JSON, one per line, flushed each time.
    pub fn json_lines(w: impl Write + Send + 'static) -> Self {
        let w = Mutex::new(w);
        Self::new(move |e| {
            if let Ok(mut w) = w.lock() {
                let _ = write_line(&mut *w, e);
            }
        })
    }

    /// Whether events go nowhere (nobody asked for them).
    pub fn is_silent(&self) -> bool {
        self.sink.is_none()
    }

    /// Sends one event.
    pub fn emit(&self, e: &Event) {
        if let Some(f) = &self.sink {
            f(e);
        }
    }

    /// A step's state, with a detail.
    pub fn step(&self, at: &Scope, state: State, detail: Option<String>) {
        self.emit(&Event::Step {
            target: at.target.clone(),
            machine: at.machine.clone(),
            step: at.step.clone(),
            state,
            detail,
            time: crate::registry::now(),
        });
    }

    /// A line the tool printed during a step.
    pub fn log(&self, at: &Scope, stream: Stream, line: &str) {
        self.emit(&Event::Log {
            target: at.target.clone(),
            machine: at.machine.clone(),
            step: at.step.clone(),
            stream,
            line: line.to_string(),
            time: crate::registry::now(),
        });
    }

    /// The command's outcome.
    pub fn result(&self, command: &str, ok: bool, data: serde_json::Value) {
        self.emit(&Event::Result {
            command: command.to_string(),
            ok,
            data,
            time: crate::registry::now(),
        });
    }
}

/// `value` as one line of JSON on `w`, flushed.
pub fn write_line(w: &mut dyn Write, value: &impl Serialize) -> std::io::Result<()> {
    let text = serde_json::to_string(value).map_err(std::io::Error::other)?;
    writeln!(w, "{text}")?;
    w.flush()
}

/// The JSON Schema of a line of `--json` output: an event or an error document.
pub fn schema() -> serde_json::Value {
    let mut s = serde_json::to_value(schemars::schema_for!(Line)).unwrap_or_default();
    s["$id"] = serde_json::json!(EVENTS_SCHEMA_URL);
    s["title"] = serde_json::json!("Isoloom JSON events and errors");
    s["description"] = serde_json::json!(
        "One line of an Isoloom lifecycle command's --json output (newline-delimited JSON): an event, or the error document that ends a failed command."
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_has_the_documented_shape() {
        let e = Event::Step {
            target: Some("docker".into()),
            machine: None,
            step: "up".into(),
            state: State::Running,
            detail: None,
            time: "2026-10-14T10:00:00Z".into(),
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "event": "step", "target": "docker", "machine": null, "step": "up", "state": "running", "detail": null, "time": "2026-10-14T10:00:00Z" })
        );
    }

    #[test]
    fn errors_have_stable_codes() {
        let doc = ErrorDocument {
            error: ErrorBody::new(ErrorCode::HostNotReady, "no docker"),
        };
        let v = serde_json::to_value(&doc).unwrap();
        assert_eq!(v["error"]["code"], "host_not_ready");
        assert_eq!(ErrorCode::HostNotReady.as_str(), "host_not_ready");
        let back: Line = serde_json::from_value(v).unwrap();
        assert_eq!(back, Line::Error(doc));
    }

    #[test]
    fn what_a_newer_isoloom_adds_is_read_not_refused() {
        let line: Line = serde_json::from_str(r#"{"event":"progress","percent":50}"#).unwrap();
        assert_eq!(line, Line::Event(Event::Unknown));
        let line: Line =
            serde_json::from_str(r#"{"error":{"code":"quota_exceeded","message":"m","machine":null,"step":null,"hint":null,"problems":[]}}"#).unwrap();
        assert!(matches!(line, Line::Error(ErrorDocument { error }) if error.code == ErrorCode::Unknown));
    }

    #[test]
    fn a_channel_receives_what_is_emitted() {
        let (em, rx) = Emitter::channel();
        em.step(&Scope::step(Some("docker"), "up").on("web"), State::Done, None);
        drop(em);
        let got: Vec<Event> = rx.iter().collect();
        assert!(matches!(&got[..], [Event::Step { machine: Some(m), state: State::Done, .. }] if m == "web"));
    }

    #[test]
    fn json_lines_are_one_object_per_line() {
        struct Shared(Arc<Mutex<Vec<u8>>>);
        impl Write for Shared {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().write(b)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let buf = Arc::new(Mutex::new(Vec::new()));
        let em = Emitter::json_lines(Shared(buf.clone()));
        em.log(&Scope::step(None, "up"), Stream::Stderr, "pulling");
        em.result("run", true, serde_json::json!({}));
        let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        let lines: Vec<Line> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn the_published_schema_is_up_to_date() {
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/events.schema.json");
        let published: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(
            published,
            schema(),
            "regenerate it: cargo run -q -p isoloom -- schema --events > docs/events.schema.json"
        );
    }

    #[test]
    fn the_schema_names_every_event_and_the_error() {
        let text = schema().to_string();
        for kind in ["step", "log", "check", "result", "spec_invalid", "host_not_ready"] {
            assert!(text.contains(&format!("\"{kind}\"")), "{kind} missing from the schema");
        }
    }
}
