//! `isoloom tc show|set|disable|reset <network>`: netem on a running environment (Docker and
//! local VMs), wherever the spec's `tc` applies: the router's interface into the network and
//! each machine's own interface on it.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use isoloom_core as core;
use isoloom_core::{Target, shell};

use crate::machine::find_interface;
use crate::tools::compose;
use crate::tools::process::tool_error;
use crate::{Res, output, spec, target};

#[derive(clap::Args)]
pub struct Args {
    /// `show`, `set`, `disable` or `reset`.
    action: String,
    /// The network (one Isoloom's router is on).
    network: String,
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// The target it runs on (default: what `run` recorded for this folder).
    #[arg(long)]
    target: Option<String>,
    /// The instance.
    #[arg(long, value_name = "N")]
    instance: Option<u8>,
    /// For `set`: one-way delay, e.g. 50ms.
    #[arg(long)]
    delay: Option<String>,
    /// For `set`: delay variation, e.g. 5ms.
    #[arg(long)]
    jitter: Option<String>,
    /// For `set`: loss in percent.
    #[arg(long)]
    loss: Option<f64>,
    /// For `set`: rate cap, e.g. 10mbit.
    #[arg(long)]
    rate: Option<String>,
}

/// Where impairment applies: who it is, the address its interface on the network has, and
/// where to run a command there.
struct Place {
    who: String,
    addr: String,
    host: String,
}

pub fn run(a: Args) -> Res<ExitCode> {
    let dir = spec::abs(&a.dir)?;
    let spec = spec::load_as(&dir, a.instance)?;
    let (t, _) = target::pick(&dir, a.target.as_deref(), a.instance, &spec)?;
    let network = a.network.as_str();
    let net = spec.networks.get(network).ok_or_else(|| format!("no network named `{network}`"))?;
    if let Some(gw) = &net.gateway {
        return Err(format!("`{network}` is routed by its gateway machine `{gw}`, not by Isoloom's router").into());
    }
    let docker = matches!(t, Target::Docker | Target::Hosted);
    if !docker && !matches!(t, Target::Vagrant | Target::Hybrid) {
        return Err(format!("`tc` on {} comes next (Docker and local VMs today)", t.id()).into());
    }
    let places = places(&spec, a.instance, network, docker);
    if places.is_empty() {
        return Err(format!("nothing on `{network}` to impair").into());
    }
    let wanted = core::Tc {
        delay: a.delay,
        jitter: a.jitter,
        loss: a.loss,
        rate: a.rate,
    };
    let mut ok = true;
    for p in places {
        let command = tc_command(&a.action, &p, network, &wanted, net.tc.as_ref())?;
        let mut c = if docker {
            let mut c = compose::quiet(&dir, a.instance);
            c.args(["exec", "-T", &p.host, "sh", "-c", &command]);
            c
        } else {
            let mut c = Command::new("vagrant");
            c.current_dir(dir.join(core::instance::output_dir(a.instance)).join(target::vm_folder(t)));
            c.args(["ssh", &p.host, "-c", &format!("sudo sh -c {}", shell::quote(&command))]);
            c
        };
        ok &= c.status().map_err(|e| tool_error(&c, e))?.success();
    }
    Ok(output::exit_code(ok))
}

/// The router's interface into the network, and each machine's own on it.
fn places(spec: &core::Spec, instance: Option<u8>, network: &str, docker: bool) -> Vec<Place> {
    let r = core::resolved::resolve_with(spec, instance);
    let mut at = Vec::new();
    if let Some(a) = core::resolved::lookup(&r, &format!("networks.{network}.router")).and_then(|v| v.as_str()) {
        let router = core::generate::router_name().to_string();
        at.push(Place {
            who: router.clone(),
            addr: a.to_string(),
            host: router,
        });
    }
    for (name, m) in &spec.machines {
        let windows = m.docker.is_none() && m.vm.as_ref().is_some_and(|v| v.os.starts_with("windows"));
        if !m.networks.contains_key(network) || windows {
            continue;
        }
        let key = if docker { "docker_addresses" } else { "addresses" };
        let Some(a) = core::resolved::lookup(&r, &format!("machines.{name}.{key}.{network}")).and_then(|v| v.as_str()) else {
            continue;
        };
        // On Docker, the machine's network sidecar has `tc` and the right to use it.
        let host = if docker { format!("{name}-routes") } else { name.clone() };
        if docker && m.docker.is_none() {
            continue;
        }
        at.push(Place {
            who: name.clone(),
            addr: a.to_string(),
            host,
        });
    }
    at
}

/// The shell command the action runs at one place.
fn tc_command(action: &str, p: &Place, network: &str, wanted: &core::Tc, spec_tc: Option<&core::Tc>) -> Res<String> {
    let find = find_interface(&p.addr, &format!("no interface on {network}"));
    let who = &p.who;
    Ok(match action {
        "show" => format!("{find}; echo \"{who}:\"; tc qdisc show dev \"$IF\""),
        "disable" => format!("{find}; tc qdisc del dev \"$IF\" root 2>/dev/null; echo \"{who} on {network}: impairment off\""),
        "set" => {
            let netem = wanted.netem();
            if netem.is_empty() {
                return Err("`set` takes --delay, --jitter, --loss and/or --rate".into());
            }
            format!("{find}; tc qdisc replace dev \"$IF\" root netem {netem} && echo \"{who} on {network}: {netem}\"")
        }
        "reset" => match spec_tc {
            Some(tc) => format!(
                "{find}; tc qdisc replace dev \"$IF\" root netem {0} && echo \"{who} on {network}: {0} (the spec's)\"",
                tc.netem()
            ),
            None => format!("{find}; tc qdisc del dev \"$IF\" root 2>/dev/null; echo \"{who} on {network}: the spec sets no impairment; off\""),
        },
        other => return Err(format!("unknown action `{other}`: show, set, disable or reset").into()),
    })
}
