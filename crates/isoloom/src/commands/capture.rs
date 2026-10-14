//! `isoloom capture <machine> <network> [-- tcpdump args]`: tcpdump on the machine's interface
//! on that network, found by its address from inside the machine's network namespace (so it
//! works the same on Docker Desktop, Linux, local VMs and cloud VMs).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use isoloom_core as core;
use isoloom_core::{Target, shell};

use crate::machine::{Running, find_interface, on_machine};
use crate::tools::{compose, process};
use crate::{Res, spec};

#[derive(clap::Args)]
#[allow(clippy::struct_field_names)] // `args`: tcpdump's own arguments, named as `--help` shows them.
pub struct Args {
    /// The machine.
    machine: String,
    /// The network (one of the machine's).
    network: String,
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
    /// tcpdump arguments, after `--` (default: `-l -v`).
    #[arg(last = true)]
    args: Vec<String>,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let dir = spec::abs(&a.dir)?;
    let running = Running::find(&dir, a.target.as_deref(), a.instance)?;
    let (machine, network, t) = (a.machine.as_str(), a.network.as_str(), running.target);
    let m = running.spec.machines.get(machine).ok_or_else(|| format!("no machine named `{machine}`"))?;
    if !m.networks.contains_key(network) {
        return Err(format!(
            "`{machine}` isn't on `{network}` (its networks: {})",
            m.networks.keys().cloned().collect::<Vec<_>>().join(", ")
        )
        .into());
    }
    let addr = address(&running.spec, t, machine, network)?;
    let extra = if a.args.is_empty() {
        "-l -v".to_string()
    } else {
        a.args.iter().map(|x| shell::quote(x)).collect::<Vec<_>>().join(" ")
    };
    let find = find_interface(&addr, &format!("no interface with {addr} in this machine"));
    let mut c = match t {
        Target::Docker | Target::Hosted => {
            // The machine's image rarely has tcpdump: a netshoot container in its network namespace.
            if m.docker.is_none() {
                return Err(format!("`{machine}` has no `docker:`: on Docker it is supplied by the runner").into());
            }
            netshoot(&dir, a.instance, machine, &format!("{find}; exec tcpdump -i \"$IF\" {extra}"))?
        }
        Target::Vagrant | Target::CloudVm => {
            let tcpdump = format!(
                "command -v tcpdump >/dev/null 2>&1 || {{ echo 'tcpdump is not installed in {machine} (apt-get install tcpdump)' >&2; exit 1; }}; {find}; exec tcpdump -i \"$IF\" {extra}"
            );
            let env = running.env(&dir, a.instance, a.ssh_key.as_deref());
            on_machine(&env, machine, Some(&tcpdump), process::terminal(), true)?
        }
        other => return Err(format!("capture on {} comes next (Docker, local VMs and cloud VMs today)", other.id()).into()),
    };
    process::run_attached(&mut c)
}

/// The machine's address on the network, where the target puts it.
fn address(spec: &core::Spec, t: Target, machine: &str, network: &str) -> Res<String> {
    let r = core::resolved::resolve(spec);
    let on_docker = matches!(t, Target::Docker | Target::Hosted | Target::DockerVm | Target::CloudDocker | Target::Kubernetes);
    let key = if on_docker { "docker_addresses" } else { "addresses" };
    Ok(core::resolved::lookup(&r, &format!("machines.{machine}.{key}.{network}"))
        .and_then(|v| v.as_str())
        .ok_or("no address")?
        .to_string())
}

/// A netshoot container in the network namespace of the machine's running container.
fn netshoot(dir: &Path, instance: Option<u8>, machine: &str, script: &str) -> Res<Command> {
    let id = Command::new("docker")
        .arg("compose")
        .args(compose::compose_files(dir, instance))
        .args(["ps", "-q", machine])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("docker: {e}"))?;
    let id = String::from_utf8_lossy(&id.stdout).trim().to_string();
    if id.is_empty() {
        return Err(format!("`{machine}` isn't running (`isoloom status`)").into());
    }
    let mut c = Command::new("docker");
    // No terminal: tcpdump only writes, and Docker forwards Ctrl-C to it anyway.
    c.args(["run", "--rm", "--net", &format!("container:{id}"), "nicolaka/netshoot", "sh", "-c", script]);
    c.current_dir(dir);
    Ok(c)
}
