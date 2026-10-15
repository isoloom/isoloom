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
                    p.add(format!("provision[{i}].groups.{group}[{j}]"), format!("no machine named `{member}`"));
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
            p.add(format!("provision[{i}].host_vars.{machine}"), format!("no machine named `{machine}`"));
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
