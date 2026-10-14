//! Running the check runners: their PASS/FAIL lines read (whatever prefix the tool adds), the
//! rest of their output streamed, and the results summed up. With `--json`, each result is a
//! `check` event, the rest of the output `log` events, and the sum the `result` event.

use std::io::BufRead;
use std::process::{ExitCode, Stdio};

use isoloom_core::Target;
use isoloom_core::checks::{self, Line};
use isoloom_core::events::{Event, Scope, Stream};

use super::runner::{Runner, STARTING_JOBS};
use crate::tools::process;
use crate::{Res, output};

/// One check's result.
#[derive(serde::Serialize)]
pub struct Outcome {
    position: String,
    name: String,
    ok: bool,
    detail: String,
}

/// Every check's result, and the runners that didn't finish.
#[derive(Default)]
pub struct Tally {
    outcomes: Vec<Outcome>,
    broken: Vec<String>,
}

/// Runs each runner in turn.
pub fn run_all(runners: &[Runner], t: Target) -> Res<Tally> {
    let mut tally = Tally::default();
    for r in runners {
        let at = Scope::step(Some(t.id()), "checks");
        if output::json() {
            output::begin(&at, Some(r.label.clone()));
        } else {
            println!("{}", r.label);
        }
        let ended = run_one(r, &at, &mut tally.outcomes)?;
        output::done(&at, Some(r.label.clone()));
        if !ended && r.label != STARTING_JOBS && !(matches!(t, Target::Vagrant | Target::DockerVm | Target::Hybrid)) {
            tally.broken.push(r.label.clone());
        }
    }
    Ok(tally)
}

/// Runs one runner, reading its lines; whether it reported its end.
fn run_one(r: &Runner, at: &Scope, outcomes: &mut Vec<Outcome>) -> Res<bool> {
    let mut cmd = process::command(&r.program);
    cmd.args(&r.args).current_dir(&r.wd).stdout(Stdio::piped());
    for (k, v) in &r.env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().map_err(|e| process::spawn_error(&r.program, &e))?;
    let stdout = child.stdout.take().ok_or("the runner has no output")?;
    let mut ended = false;
    for line in std::io::BufReader::new(stdout).lines() {
        ended |= read_line(&line?, at, &r.label, outcomes);
    }
    let _ = child.wait();
    Ok(ended)
}

/// One line of a runner: a result recorded (and printed, or sent as an event), its end, or
/// output streamed. True when it is the runner's end.
fn read_line(line: &str, at: &Scope, label: &str, outcomes: &mut Vec<Outcome>) -> bool {
    let json = output::json();
    // Vagrant prefixes each VM's output with its name: it says which machine ran the check.
    let position = line
        .split_once("isoloom-check: ")
        .map(|(pre, _)| pre.trim().trim_end_matches(':').to_string())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| label.to_string());
    let (name, ok, detail) = match checks::parse_line(line) {
        Some(Line::Pass(name)) => (name, true, String::new()),
        Some(Line::Fail(name, why)) => (name, false, why),
        Some(Line::End(_, _)) => return true,
        None => {
            if json {
                output::events().log(at, Stream::Stdout, line);
            } else if !line.trim().is_empty() {
                println!("    {line}");
            }
            return false;
        }
    };
    let outcome = Outcome { position, name, ok, detail };
    report(at, &outcome);
    outcomes.push(outcome);
    false
}

/// One result: a line on the terminal, or a `check` event.
fn report(at: &Scope, o: &Outcome) {
    if output::json() {
        output::events().emit(&Event::Check {
            target: at.target.clone(),
            position: o.position.clone(),
            name: o.name.clone(),
            ok: o.ok,
            detail: o.detail.clone(),
            time: isoloom_core::registry::now(),
        });
    } else if o.ok {
        println!("  ✓ {}", o.name);
    } else {
        println!("  ✗ {}: {}", o.name, o.detail);
    }
}

/// Prints the results (or the `result` event), and fails unless every check passed and every
/// runner finished.
pub fn summarize(t: Target, tally: &Tally) -> ExitCode {
    let Tally { outcomes, broken } = tally;
    let passed = outcomes.iter().filter(|o| o.ok).count();
    let failed = outcomes.iter().filter(|o| !o.ok).count();
    let ok = failed == 0 && broken.is_empty() && !outcomes.is_empty();
    if output::json() {
        output::result(
            "test",
            ok,
            serde_json::json!({
                "target": t.id(),
                "passed": passed,
                "failed": failed,
                "unfinished": broken,
                "results": outcomes,
            }),
        );
    } else {
        println!();
        if failed == 0 && broken.is_empty() {
            println!("{passed} passed");
        } else {
            println!("{passed} passed, {failed} failed");
        }
        for b in broken {
            println!("✗ the runner {b} didn't finish (see its output above)");
        }
    }
    output::exit_code(ok)
}
