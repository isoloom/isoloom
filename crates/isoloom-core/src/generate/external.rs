//! The `external` target: machines that already exist (a hand-built Proxmox or ESXi set of
//! VMs, a home lab, hardware). Isoloom creates nothing; it provisions, checks and reaches them
//! over SSH at the addresses the spec gives (`machines.*.external`), in `.isoloom/external/`:
//!
//! - `inventory.ini`: every machine with its `external` address, user, port and key, in the
//!   `linux` / `windows` groups and the spec's groups, for `provision:` playbooks run from here.
//! - `checks/<machine>.sh`: the check runner of each machine that has an address (declared and
//!   derived probes; scripts need the project on the machine and are left out, with a note).
//! - `machines.json`: name -> address, user, port, key, for `isoloom run/test/connect/exec`.
//!
//! Networks and `reach` are expected behavior here (Isoloom adds no router), which the derived
//! checks verify.

use std::fmt::Write;

use serde_json::json;

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, address, header};
use crate::checks::{self, Probe};
use crate::images;
use crate::model::{Spec, Target};

const DIR: &str = "external";

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    let mut files = Vec::new();

    // The inventory, as the controller writes it on the VM targets, but reached from here.
    let mut linux = String::new();
    let mut windows = String::new();
    for (name, m) in &spec.machines {
        let Some(e) = &m.external else { continue };
        let mut line = format!("{name} ansible_host={}", e.address);
        if let Some(u) = &e.user {
            let _ = write!(line, " ansible_user={u}");
        }
        if let Some(p) = e.port {
            let _ = write!(line, " ansible_port={p}");
        }
        if let Some(k) = &e.key {
            let _ = write!(line, " ansible_ssh_private_key_file={k}");
        }
        line.push_str(&super::host_vars(spec, name));
        if m.vm.as_ref().is_some_and(|v| images::is_windows(&v.os)) {
            let _ = writeln!(windows, "{line}");
        } else {
            let _ = writeln!(linux, "{line}");
        }
    }
    let mut inv = format!("{}[linux]\n{linux}\n[windows]\n{windows}\n[linux:vars]\nansible_become=true\n", header("#"));
    if !windows.is_empty() {
        inv.push_str("\n[windows:vars]\nansible_connection=ssh\nansible_shell_type=powershell\n");
    }
    for g in spec.groups.keys() {
        let members: Vec<String> = crate::groups::members(spec, g)
            .into_iter()
            .filter(|m| spec.machines[m].external.is_some())
            .collect();
        if !members.is_empty() {
            let _ = write!(inv, "\n[{g}]\n{}\n", members.join("\n"));
        }
    }
    for step in &spec.provision {
        for (g, members) in &step.groups {
            let _ = write!(inv, "\n[{g}]\n{}\n", members.join("\n"));
        }
    }
    files.push(GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/inventory.ini"),
        contents: inv,
    });

    // The machines, for the CLI.
    let machines: serde_json::Map<String, serde_json::Value> = spec
        .machines
        .iter()
        .filter_map(|(n, m)| {
            m.external.as_ref().map(|e| {
                (
                    n.clone(),
                    json!({ "address": e.address, "user": e.user.clone().unwrap_or_else(|| "root".into()), "port": e.port.unwrap_or(22), "key": e.key }),
                )
            })
        })
        .collect();
    files.push(GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/machines.json"),
        contents: format!("{}\n", serde_json::to_string_pretty(&serde_json::Value::Object(machines)).expect("serializes")),
    });

    // Check runners: one per machine with an address; probes only (no project on the machine).
    let plan = checks::plan(spec);
    let host = |h: &checks::Host, _: &checks::Position| -> String {
        match h {
            checks::Host::Literal(l) => l.clone(),
            checks::Host::Machine { name, network } => address(spec, network, spec.machines[name].networks[network]).to_string(),
        }
    };
    let no_script = |_: &str| "echo 'scripts need the project on the machine; the external target runs probes only' >&2; exit 1".to_string();
    let render = checks::Render {
        host: &host,
        script: &no_script,
        playbook: None,
    };
    for (pos, group) in checks::by_position(spec, &plan) {
        let checks::Position::Machine(m) = &pos else { continue };
        if spec.machines[m].external.is_none() {
            continue;
        }
        let probes: Vec<&checks::Resolved> = group
            .into_iter()
            .filter(|c| !matches!(c.probe, Probe::Script { .. } | Probe::Playbook { .. }))
            .collect();
        if probes.is_empty() {
            continue;
        }
        files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/checks/{m}.sh"),
            contents: checks::script(&pos, &probes, &render),
        });
    }
    let _ = Target::External;
    Ok(files)
}
