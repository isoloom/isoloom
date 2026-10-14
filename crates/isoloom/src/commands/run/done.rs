//! What follows a successful `run` or `down`: the registry updated, and after a run the
//! baseline snapshot, the warnings the tool's output loses, and the spec's message.

use isoloom_core as core;
use isoloom_core::Target;

use super::Ask;
use super::Invocation;
use crate::prepare::Prepared;
use crate::tools::compose;
use crate::tools::vagrant::{self, BASELINE};
use crate::{cloud_services, spec};

/// Everything after the tool succeeded.
pub fn after(p: &Prepared, ask: &Ask, down: bool, tool: &Invocation, project: Option<String>) {
    record(p, ask, down, project);
    if down {
        return;
    }
    // VMs: a baseline snapshot of the environment as it came up, for `isoloom reset`.
    if p.target == Target::Vagrant && std::env::var_os("ISOLOOM_NO_BASELINE").is_none() {
        save_baseline(tool);
    }
    // vagrant-libvirt skips UDP forwarded ports (a warning lost in Vagrant's output).
    if tool.program == "vagrant" && vagrant::vagrant_uses_libvirt(&tool.wd) {
        for line in vagrant::udp_forwards(&p.spec) {
            eprintln!("warning: vagrant-libvirt doesn't forward UDP ports: {line} isn't reachable from this host (use the machine's address)");
        }
    }
    print_message(p, ask.instance);
}

/// Remember what is up on this host, for status / connect / exec / capture.
fn record(p: &Prepared, ask: &Ask, down: bool, project: Option<String>) {
    let recorded = core::registry::update(|reg| {
        if down {
            reg.remove(&p.dir, p.target, ask.instance);
        } else {
            reg.upsert(core::registry::Entry {
                name: p.spec.name.clone(),
                dir: p.dir.clone(),
                target: p.target,
                instance: ask.instance,
                cloud: ask.cloud.map(str::to_string),
                project,
                profile: spec::profile_name(),
                started: core::registry::now(),
            });
        }
    });
    if let Err(e) = recorded {
        eprintln!("note: couldn't update {}: {e}", core::registry::path().display());
    }
}

fn save_baseline(tool: &Invocation) {
    eprintln!("Saving the baseline snapshot ({BASELINE}) for `isoloom reset`");
    let saved = std::process::Command::new("vagrant")
        .args(["snapshot", "save", "--force", BASELINE])
        .current_dir(&tool.wd)
        .status()
        .is_ok_and(|s| s.success());
    if !saved {
        eprintln!("note: no baseline snapshot (the provider may not support snapshots); `reset` will rebuild");
    }
}

/// The spec's message, now that the environment is up.
fn print_message(p: &Prepared, instance: Option<u8>) {
    if p.target == Target::CloudServices {
        if let Some(m) = &p.spec.cloud_message(&cloud_services::outputs(&p.dir, instance)) {
            println!("\n{}", m.trim_end());
        }
    } else if let Ok(Some(m)) = core::resolved::render_message_at(&p.spec, instance, p.target, &compose::real_ports(&p.dir, p.target, instance)) {
        println!("\n{}", m.trim_end());
    }
}
