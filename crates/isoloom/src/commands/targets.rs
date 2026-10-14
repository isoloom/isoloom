//! `isoloom targets`: the targets the spec can run on, and why the others can't.

use std::path::PathBuf;
use std::process::ExitCode;

use isoloom_runtime::host::Readiness;
use isoloom_spec::Target;

use crate::{Res, output, spec};

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    dir: PathBuf,
    #[arg(long)]
    json: bool,
    /// Also say whether this machine can run each target (tools installed, credentials
    /// present), and what is missing.
    #[arg(long)]
    host: bool,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let spec = spec::load_spec(&a.dir)?;
    if spec::report_invalid(&spec) {
        return Ok(ExitCode::FAILURE);
    }
    if a.json {
        print_json(&spec, a.host)?;
    } else {
        print_text(&spec, a.host);
    }
    Ok(ExitCode::SUCCESS)
}

/// With --host, whether this machine has what each target needs (one line per cloud).
fn on_host(t: Target, host: bool) -> Vec<Readiness> {
    if !host {
        return vec![];
    }
    match t {
        Target::CloudDocker | Target::CloudVm => isoloom_runtime::host::CLOUDS.iter().map(|c| isoloom_runtime::host::check(t, Some(c))).collect(),
        _ => vec![isoloom_runtime::host::check(t, None)],
    }
}

/// The targets `generate` really produces: possible by the machines' editions, and not
/// refused by the generator.
fn ready(spec: &isoloom_spec::Spec) -> Vec<Target> {
    isoloom_spec::effective(spec)
        .into_iter()
        .filter(|t| isoloom_targets::refusal(spec, *t).is_none())
        .collect()
}

fn print_json(spec: &isoloom_spec::Spec, host: bool) -> Res<()> {
    let ready = ready(spec);
    if !host {
        let ids: Vec<&str> = ready.iter().map(|t| t.id()).collect();
        return output::print_json(&ids);
    }
    let list: Vec<serde_json::Value> = ready
        .iter()
        .map(|t| {
            let checks: Vec<serde_json::Value> = on_host(*t, host)
                .iter()
                .map(|r| serde_json::json!({ "cloud": r.cloud, "ready": r.ready, "notes": r.notes }))
                .collect();
            serde_json::json!({ "target": t.id(), "host": checks })
        })
        .collect();
    output::print_json(&list)
}

fn print_text(spec: &isoloom_spec::Spec, host: bool) {
    let effective = isoloom_spec::effective(spec);
    for t in Target::ALL {
        if !effective.contains(&t) {
            println!("✗ {} ({})", t.id(), why_not(spec, t));
            continue;
        }
        // A target can be possible by its machines' editions and still be refused by its
        // generator (a Windows machine on Proxmox, say). A ✓ here means `generate` really
        // produces it; a refusal shows as ✗ with the generator's reason.
        match isoloom_targets::refusal(spec, t) {
            None => {
                println!("✓ {}", t.id());
                // A cloud whose model doesn't fit drops its module, not the target.
                for (cloud, why) in isoloom_targets::cloud_refusals(spec, t).map(|(_, r)| r).unwrap_or_default() {
                    println!("    ✗ {cloud} ({why})");
                }
                for r in on_host(t, host) {
                    let label = r.cloud.as_deref().map(|c| format!(" {c}")).unwrap_or_default();
                    println!("    host{label}: {}", r.summary());
                }
            }
            Some(why) => println!("✗ {} ({why})", t.id()),
        }
    }
}

/// Why the spec can't run on a target at all.
fn why_not(spec: &isoloom_spec::Spec, t: Target) -> String {
    let lacking = isoloom_spec::targets::missing(spec, t.needs());
    if spec.cloud.is_some() && t != Target::CloudServices {
        "cloud services run on `cloud-services` only".to_string()
    } else if t == Target::CloudServices {
        "needs `cloud:` (cloud services instead of machines)".to_string()
    } else if !lacking.is_empty() {
        format!("needs `{}:` on {}", t.needs().key(), lacking.join(", "))
    } else if t == Target::Hybrid && !isoloom_spec::targets::mixed(spec) {
        "only when some machines are containers and others are VMs".to_string()
    } else {
        "left out by `targets:`".to_string()
    }
}
