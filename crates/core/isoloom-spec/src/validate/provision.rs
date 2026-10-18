//! Environment-level provisioning (`provision:`) and the controller VM that runs it.

use super::{Problems, variable_name};
use crate::model::{Provision, Spec};

/// The controller is a Linux VM Isoloom drives over SSH, big enough for Ansible.
pub(super) fn controller(spec: &Spec, p: &mut Problems) {
    let Some(c) = &spec.controller else { return };
    if c.image.as_ref().is_some_and(|i| i.winrm.is_some()) {
        p.add("controller.image.winrm", "the controller is a Linux VM: no WinRM");
    }
    if let Some(r) = &c.resources {
        if r.memory_mb.is_some_and(|m| m < 256) {
            p.add("controller.resources.memory_mb", "at least 256 MB (Ansible and Python)");
        }
        if r.cpus == Some(0) {
            p.add("controller.resources.cpus", "at least 1");
        }
    }
}

/// Each step: a playbook, groups of the spec's machines (not Isoloom's own groups), and host
/// variables for machines that exist.
pub(super) fn steps(spec: &Spec, p: &mut Problems) {
    for (i, step) in spec.provision.iter().enumerate() {
        if !crate::model::is_yaml_file(&step.ansible) {
            p.add(format!("provision[{i}].ansible"), "an Ansible playbook: a .yml or .yaml file");
        }
        for (group, members) in &step.groups {
            if matches!(group.as_str(), "all" | "linux" | "windows") {
                p.add(format!("provision[{i}].groups.{group}"), "Isoloom fills this group itself");
            }
            for (j, member) in members.iter().enumerate() {
                if !spec.machines.contains_key(member) {
                    p.unknown(format!("provision[{i}].groups.{group}[{j}]"), format!("no machine named `{member}`"));
                }
            }
        }
        host_vars(spec, i, step, p);
    }
}

/// A step's `host_vars`: machines of the spec, variables Ansible accepts.
fn host_vars(spec: &Spec, i: usize, step: &Provision, p: &mut Problems) {
    for (machine, vars) in &step.host_vars {
        if !spec.machines.contains_key(machine) {
            p.unknown(format!("provision[{i}].host_vars.{machine}"), format!("no machine named `{machine}`"));
        }
        for k in vars.keys() {
            if !variable_name(k) {
                p.add(
                    format!("provision[{i}].host_vars.{machine}.{k}"),
                    "a variable name: letters, digits and `_`, not starting with a digit",
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    /// The problems of VMs `web` and `db` with `rest` (a `controller`, `provision`).
    fn with(rest: &str) -> Vec<String> {
        problems(&lab(&format!(
            "machines:\n  web: {{ networks: {{ lan: 10 }}, vm: {{ os: debian@12 }} }}\n  db: {{ networks: {{ lan: 11 }}, vm: {{ os: debian@12 }} }}\n{rest}\n"
        )))
    }

    #[test]
    fn a_sound_controller_and_steps_pass() {
        let ok = "controller: { resources: { memory_mb: 256, cpus: 1 } }\nprovision: [{ ansible: site.yaml, groups: { front: [web] }, host_vars: { db: { db_port: 5432, _x1: y } } }]";
        assert_eq!(with(ok), Vec::<String>::new());
    }

    #[test]
    fn the_controller_has_the_room_ansible_needs() {
        assert_eq!(
            with("controller: { resources: { memory_mb: 255, cpus: 0 } }"),
            [
                "controller.resources.memory_mb: at least 256 MB (Ansible and Python)",
                "controller.resources.cpus: at least 1"
            ]
        );
    }

    #[test]
    fn a_step_runs_a_playbook_on_the_specs_machines() {
        let step = "provision: [{ ansible: site.sh, groups: { linux: [web], front: [web, cache] }, host_vars: { cache: { 1x: y } } }]";
        assert_eq!(
            with(step),
            [
                "provision[0].ansible: an Ansible playbook: a .yml or .yaml file",
                "provision[0].groups.linux: Isoloom fills this group itself",
                "provision[0].groups.front[1]: no machine named `cache`",
                "provision[0].host_vars.cache: no machine named `cache`",
                "provision[0].host_vars.cache.1x: a variable name: letters, digits and `_`, not starting with a digit",
            ]
        );
    }
}
