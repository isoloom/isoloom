//! `isoloom connect <machine>`: a shell on a machine of a running environment.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::machine::{Running, on_machine};
use crate::tools::process;
use crate::{Res, spec};

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
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let dir = spec::abs(&a.dir)?;
    let running = Running::find(&dir, a.target.as_deref(), a.instance)?;
    let machine = &a.machine;
    if let Some(clones) = running.spec.clones.get(machine) {
        return Err(format!("`{machine}` is {} machines; connect to one of them: {}", clones.len(), clones.join(", ")).into());
    }
    let env = running.env(&dir, a.instance, a.ssh_key.as_deref());
    let mut c = on_machine(&env, machine, None, process::terminal(), false)?;
    process::run_attached(&mut c)
}
