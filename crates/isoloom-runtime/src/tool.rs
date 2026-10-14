//! Running the tool a target works with (docker compose, vagrant, terraform, kubectl, ssh) with
//! its output turned into `log` events, for a caller that reads events rather than a terminal.

use std::io::{BufRead, BufReader, Read};
use std::process::{Command, ExitStatus, Stdio};

use crate::events::{Emitter, Scope, Stream};

/// Runs `cmd` to the end, each line it prints sent to `emitter` as a `log` event of `at`
/// (standard output and standard error alike, told apart by `stream`). Its standard input is
/// left as the caller set it.
pub fn run_logged(cmd: &mut Command, emitter: &Emitter, at: &Scope) -> std::io::Result<ExitStatus> {
    run_logged_input(cmd, None, emitter, at)
}

/// [`run_logged`], with `input` written to the tool's standard input (then closed) when given:
/// a script for a remote shell, without quoting it through a local one.
pub fn run_logged_input(cmd: &mut Command, input: Option<&[u8]>, emitter: &Emitter, at: &Scope) -> std::io::Result<ExitStatus> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    if input.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn()?;
    let writer = feed(&mut child, input);
    let stderr = child.stderr.take().map(|err| {
        let (emitter, at) = (emitter.clone(), at.clone());
        std::thread::spawn(move || forward(err, &emitter, &at, Stream::Stderr))
    });
    if let Some(out) = child.stdout.take() {
        forward(out, emitter, at, Stream::Stdout);
    }
    if let Some(t) = stderr {
        let _ = t.join();
    }
    if let Some(w) = writer {
        let _ = w.join();
    }
    child.wait()
}

/// Writes `input` to the child's standard input on a thread of its own (a tool may print before
/// it reads, and its output is read meanwhile), closing it after.
pub fn feed(child: &mut std::process::Child, input: Option<&[u8]>) -> Option<std::thread::JoinHandle<()>> {
    let (mut stdin, input) = (child.stdin.take()?, input?.to_vec());
    Some(std::thread::spawn(move || {
        let _ = std::io::Write::write_all(&mut stdin, &input);
    }))
}

/// Every line of `from` as a `log` event, until it closes. Bytes that aren't UTF-8 are replaced.
fn forward(from: impl Read, emitter: &Emitter, at: &Scope, stream: Stream) {
    let mut reader = BufReader::new(from);
    let mut buf = Vec::new();
    while reader.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
        let line = String::from_utf8_lossy(&buf);
        emitter.log(at, stream, line.trim_end_matches(['\n', '\r']));
        buf.clear();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::events::Event;

    #[test]
    fn both_outputs_become_log_events() {
        let (em, rx) = Emitter::channel();
        let mut c = Command::new("sh");
        c.args(["-c", "echo out; echo err >&2; exit 3"]);
        let status = run_logged(&mut c, &em, &Scope::step(Some("docker"), "up")).unwrap();
        drop(em);
        assert_eq!(status.code(), Some(3));
        let mut lines: Vec<(Stream, String)> = rx
            .iter()
            .filter_map(|e| match e {
                Event::Log { stream, line, .. } => Some((stream, line)),
                _ => None,
            })
            .collect();
        lines.sort_by_key(|(s, _)| *s == Stream::Stderr);
        assert_eq!(lines, [(Stream::Stdout, "out".to_string()), (Stream::Stderr, "err".to_string())]);
    }

    #[test]
    fn input_reaches_the_tool() {
        let (em, rx) = Emitter::channel();
        let mut c = Command::new("sh");
        c.arg("-s");
        let status = run_logged_input(&mut c, Some(b"echo from-stdin\n"), &em, &Scope::step(None, "provision")).unwrap();
        drop(em);
        assert!(status.success());
        assert!(rx.iter().any(|e| matches!(e, Event::Log { line, .. } if line == "from-stdin")));
    }
}
