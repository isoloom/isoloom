//! `isoloom connect <machine>`: a shell on a machine of a running environment.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use isoloom_runtime::events::ErrorCode;

use crate::failure::Failure;
use crate::machine::{Running, on_machine};
use crate::tools::process;
use crate::{Res, output, spec};

#[derive(clap::Args)]
pub struct Args {
    /// The machine.
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
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let dir = spec::abs(&a.dir)?;
    let running = Running::find(&dir, a.target.as_deref(), a.instance)?;
    let machine = &a.machine;
    if let Some(clones) = running.spec.clones.get(machine) {
        let message = format!("`{machine}` is {} machines; connect to one of them: {}", clones.len(), clones.join(", "));
        return Err(Failure::new(ErrorCode::MachineUnknown, message).machine(machine).into());
    }
    let env = running.env(&dir, a.instance, a.ssh_key.as_deref());
    // With --json, how to open the shell (for a tool with a terminal of its own, so with one),
    // not the shell.
    let mut c = on_machine(&env, machine, None, a.json || process::terminal(), false)?;
    if a.json {
        output::print_json(&command_document(running.target.id(), machine, &c))?;
        return Ok(ExitCode::SUCCESS);
    }
    process::run_attached(&mut c)
}

/// What `connect --json` prints: the command that opens a shell on the machine, and where it runs.
fn command_document(target: &str, machine: &str, c: &Command) -> serde_json::Value {
    serde_json::json!({
        "target": target,
        "machine": machine,
        "program": c.get_program().to_string_lossy(),
        "args": c.get_args().map(|a| a.to_string_lossy()).collect::<Vec<_>>(),
        "dir": c.get_current_dir(),
    })
}
