//! Running the check runners: their PASS/FAIL lines read (whatever prefix the tool adds), the
//! rest of their output streamed, and the results summed up.

use std::io::BufRead;
use std::process::{Command, ExitCode, Stdio};

use isoloom_core::Target;
use isoloom_core::checks::{self, Line};

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
pub fn run_all(runners: &[Runner], t: Target, json: bool) -> Res<Tally> {
    let mut tally = Tally::default();
    for r in runners {
        if !json {
            println!("{}", r.label);
        }
        let ended = run_one(r, json, &mut tally.outcomes)?;
        if !ended && r.label != STARTING_JOBS && !(matches!(t, Target::Vagrant | Target::DockerVm | Target::Hybrid)) {
            tally.broken.push(r.label.clone());
        }
    }
    Ok(tally)
}

/// Runs one runner, reading its lines; whether it reported its end.
fn run_one(r: &Runner, json: bool, outcomes: &mut Vec<Outcome>) -> Res<bool> {
    let mut cmd = Command::new(&r.program);
    cmd.args(&r.args).current_dir(&r.wd).stdout(Stdio::piped());
    for (k, v) in &r.env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().map_err(|e| process::spawn_error(&r.program, &e))?;
    let stdout = child.stdout.take().ok_or("the runner has no output")?;
    let mut ended = false;
    for line in std::io::BufReader::new(stdout).lines() {
        ended |= read_line(&line?, &r.label, json, outcomes);
    }
    let _ = child.wait();
    Ok(ended)
}

/// One line of a runner: a result recorded (and printed), its end, or output streamed.
/// True when it is the runner's end.
fn read_line(line: &str, label: &str, json: bool, outcomes: &mut Vec<Outcome>) -> bool {
    // Vagrant prefixes each VM's output with its name: it says which machine ran the check.
    let position = line
        .split_once("isoloom-check: ")
        .map(|(pre, _)| pre.trim().trim_end_matches(':').to_string())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| label.to_string());
    let (name, ok, detail) = match checks::parse_line(line) {
        Some(Line::Pass(name)) => {
            if !json {
                println!("  ✓ {name}");
            }
            (name, true, String::new())
        }
        Some(Line::Fail(name, why)) => {
            if !json {
                println!("  ✗ {name}: {why}");
            }
            (name, false, why)
        }
        Some(Line::End(_, _)) => return true,
        None => {
            if !json && !line.trim().is_empty() {
                println!("    {line}");
            }
            return false;
        }
    };
    outcomes.push(Outcome { position, name, ok, detail });
    false
}

/// Prints the results (or their JSON), and fails unless every check passed and every runner
/// finished.
pub fn summarize(t: Target, tally: &Tally, json: bool) -> Res<ExitCode> {
    let Tally { outcomes, broken } = tally;
    let passed = outcomes.iter().filter(|o| o.ok).count();
    let failed = outcomes.iter().filter(|o| !o.ok).count();
    if json {
        output::print_json(&serde_json::json!({
            "target": t.id(),
            "passed": passed,
            "failed": failed,
            "unfinished": broken,
            "results": outcomes,
        }))?;
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
    Ok(output::exit_code(failed == 0 && broken.is_empty() && !outcomes.is_empty()))
}
