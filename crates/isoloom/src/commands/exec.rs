//! `isoloom exec <machine|all> -- <command>`: the command on one machine, or on every machine
//! that can be reached, each line prefixed with the machine's name.

use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use isoloom_runtime::events::ErrorCode;
use isoloom_spec::shell;

use crate::failure::fail;
use crate::machine::{Env, Running, on_machine};
use crate::tools::process;
use crate::{Res, output, spec};

#[derive(clap::Args)]
pub struct Args {
    /// The machine, or `all`.
    machine: String,
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// The target it runs on (default: what `run` recorded for this folder).
    #[arg(long)]
    target: Option<String>,
    /// The SSH private key for the cloud targets.
    #[arg(long)]
    ssh_key: Option<PathBuf>,
    /// Run as instance N (1-99) of the spec: its own names, Docker blocks and published
    /// ports, files under .isoloom-N/. For several copies on one host.
    #[arg(long, value_name = "N")]
    instance: Option<u8>,
    /// Machine-readable output: one JSON document, or the error as JSON (see docs/json.md).
    #[arg(long)]
    pub json: bool,
    /// The command, after `--`.
    #[arg(last = true)]
    command: Vec<String>,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let dir = spec::abs(&a.dir)?;
    if a.command.is_empty() {
        return Err(fail(ErrorCode::Usage, "give the command after `--`, e.g. `isoloom exec web -- uname -a`"));
    }
    let running = Running::find(&dir, a.target.as_deref(), a.instance)?;
    let cmd = quoted(&a.command);
    let env = running.env(&dir, a.instance, a.ssh_key.as_deref());
    let (spec, machine) = (&running.spec, a.machine.as_str());
    if machine != "all" && !spec.groups.contains_key(machine) && !spec.clones.contains_key(machine) {
        let mut c = on_machine(&env, machine, Some(&cmd), false, false)?;
        if a.json {
            let result = captured(machine, &mut c)?;
            let ok = result["ok"] == true;
            output::print_json(&serde_json::json!({ "target": running.target.id(), "results": [result] }))?;
            return Ok(output::exit_code(ok));
        }
        return process::run_attached(&mut c);
    }
    let names: Vec<String> = if machine == "all" {
        spec.machines.keys().cloned().collect()
    } else if let Some(clones) = spec.clones.get(machine) {
        clones.clone()
    } else {
        isoloom_spec::groups::members(spec, machine)
    };
    if a.json {
        return each_json(&env, &names, &cmd);
    }
    on_each(&env, &names, &cmd)
}

/// The command run on one machine, its output captured: `{ machine, ok, exit_code, stdout, stderr }`.
fn captured(name: &str, c: &mut Command) -> Res<serde_json::Value> {
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    let o = c.output().map_err(|e| process::tool_error(c, &e))?;
    Ok(serde_json::json!({
        "machine": name,
        "ok": o.status.success(),
        "exit_code": o.status.code(),
        "stdout": String::from_utf8_lossy(&o.stdout),
        "stderr": String::from_utf8_lossy(&o.stderr),
    }))
}

/// `exec --json` on several machines: one document with each machine's result (or why it was
/// skipped).
fn each_json(env: &Env, names: &[String], cmd: &str) -> Res<ExitCode> {
    let mut results = Vec::new();
    for name in names {
        let result = on_machine(env, name, Some(cmd), false, false).and_then(|mut c| captured(name, &mut c));
        results.push(result.unwrap_or_else(|why| serde_json::json!({ "machine": name, "skipped": why.to_string() })));
    }
    let ok = results.iter().all(|r| r["ok"] != false);
    output::print_json(&serde_json::json!({ "target": env.target.id(), "results": results }))?;
    Ok(output::exit_code(ok))
}

/// Each word quoted again for the machine's shell, so `exec web -- sh -c 'a; b'` arrives intact.
fn quoted(command: &[String]) -> String {
    command
        .iter()
        .map(|a| {
            if a.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,@%+".contains(c)) {
                a.clone()
            } else {
                shell::quote(a)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The command on each machine in turn, its output prefixed with the machine's name.
fn on_each(env: &Env, names: &[String], cmd: &str) -> Res<ExitCode> {
    let mut failed = false;
    for name in names {
        let mut c = match on_machine(env, name, Some(cmd), false, false) {
            Ok(c) => c,
            Err(why) => {
                println!("{name}: skipped ({why})");
                continue;
            }
        };
        c.stdout(Stdio::piped()).stderr(Stdio::piped());
        let o = c.output().map_err(|e| process::tool_error(&c, &e))?;
        for line in String::from_utf8_lossy(&o.stdout).lines().chain(String::from_utf8_lossy(&o.stderr).lines()) {
            println!("{name}: {line}");
        }
        if !o.status.success() {
            println!("{name}: exited with {}", o.status);
            failed = true;
        }
    }
    Ok(output::exit_code(!failed))
}
