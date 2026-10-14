//! What follows a successful `run` or `down`: the registry updated, and after a run the
//! baseline snapshot, the warnings the tool's output loses, and the spec's message.

use isoloom_core as core;
use isoloom_core::Target;

use super::Ask;
use super::Invocation;
use crate::prepare::Prepared;
use isoloom_core::events::Scope;

use crate::tools::vagrant::{self, BASELINE};
use crate::tools::{compose, process};
use crate::{cloud_services, output, spec};

/// Everything after the tool succeeded; the spec's message after a run.
pub fn after(p: &Prepared, ask: &Ask, down: bool, tool: &Invocation, project: Option<String>) -> Option<String> {
    record(p, ask, down, project);
    if down {
        return None;
    }
    // VMs: a baseline snapshot of the environment as it came up, for `isoloom reset`.
    if p.target == Target::Vagrant && std::env::var_os("ISOLOOM_NO_BASELINE").is_none() {
        save_baseline(tool);
    }
    // vagrant-libvirt skips UDP forwarded ports (a warning lost in Vagrant's output).
    if tool.program == "vagrant" && vagrant::vagrant_uses_libvirt(&tool.wd) {
        for line in vagrant::udp_forwards(&p.plan) {
            eprintln!("warning: vagrant-libvirt doesn't forward UDP ports: {line} isn't reachable from this host (use the machine's address)");
        }
    }
    let message = message(p, ask.instance);
    if let Some(m) = message.as_deref().filter(|_| !output::json()) {
        println!("\n{m}");
    }
    message
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
    let at = Scope::step(Some(Target::Vagrant.id()), "baseline");
    output::announce(
        &at,
        Some(BASELINE.to_string()),
        &format!("Saving the baseline snapshot ({BASELINE}) for `isoloom reset`"),
    );
    let mut c = process::command("vagrant");
    c.args(["snapshot", "save", "--force", BASELINE]).current_dir(&tool.wd);
    let saved = process::run_step(&mut c, &at).is_ok_and(|s| s.success());
    if saved {
        output::done(&at, None);
    } else {
        output::failed(
            &at,
            "no baseline snapshot (the provider may not support snapshots): `reset` will rebuild",
            Some("note: no baseline snapshot (the provider may not support snapshots); `reset` will rebuild"),
        );
    }
}

/// The spec's message, now that the environment is up, its trailing space trimmed.
fn message(p: &Prepared, instance: Option<u8>) -> Option<String> {
    let m = if p.target == Target::CloudServices {
        p.spec.cloud_message(&cloud_services::outputs(&p.dir, instance))
    } else {
        core::resolved::render_message_at(
            &p.spec,
            instance,
            p.target,
            &compose::real_ports(&p.dir, p.target, instance, compose::publishes(&p.plan)),
        )
        .ok()
        .flatten()
    };
    m.map(|m| m.trim_end().to_string())
}
