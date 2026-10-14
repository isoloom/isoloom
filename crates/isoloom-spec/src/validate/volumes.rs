//! Volumes: a machine's own (names Kubernetes can carry, safe mount paths) and the ones shared
//! between machines (`volumes:` at the top level).

use std::collections::BTreeSet;

use super::machines::Subject;
use super::{Problems, kebab};
use crate::model::Spec;

/// The volumes shared between machines: mounted, held by one of their users, reachable from
/// every other over a network they share (NFS on the VM targets), Linux machines only.
pub(super) fn shared(spec: &Spec, p: &mut Problems) {
    for v in spec.volumes.keys() {
        let at = format!("volumes.{v}");
        let users = spec.volume_users(v);
        if users.is_empty() {
            p.add(at, "no machine mounts it: list it in a machine's `volumes`");
            continue;
        }
        if let Some(o) = &spec.volumes[v].owner
            && !users.contains(&o.as_str())
        {
            p.add(
                format!("{at}.owner"),
                format!("`{o}` doesn't mount it: the owner is one of its machines ({})", users.join(", ")),
            );
            continue;
        }
        let owner = spec.volume_owner(v).expect("a user");
        for u in &users {
            let m = &spec.machines[*u];
            if m.vm.as_ref().is_some_and(|vm| crate::images::is_windows(&vm.os)) {
                p.add(format!("machines.{u}.volumes.{v}"), "a shared volume is mounted over NFS: Linux machines only");
            }
            if *u != owner && !m.networks.keys().any(|n| spec.machines[owner].networks.contains_key(n)) {
                p.add(
                    format!("machines.{u}.volumes.{v}"),
                    format!("`{u}` shares no network with `{owner}`, which holds the volume (NFS on the VM targets)"),
                );
            }
        }
    }
}

/// A machine's volumes: kebab-case names that fit Kubernetes' labels, and distinct, safe
/// absolute paths.
pub(super) fn of_machine(s: &Subject, p: &mut Problems) {
    let mut paths = BTreeSet::new();
    for (v, path) in &s.m.volumes {
        let vat = format!("{}.volumes.{v}", s.at);
        if let Some(why) = name_problem(s.name, v) {
            p.add(&vat, why);
        }
        // No spaces or ':' : the Docker short mount syntax (`<vol>:<path>`) and the `mkdir -p`
        // the VM/Proxmox provisioners run both split on those, so either would corrupt the mount.
        if !path.starts_with('/') || path == "/" || path.split('/').any(|s| s == "..") || path.contains([':', ' ', '\t']) {
            p.add(
                vat,
                format!("`{path}` isn't a safe absolute path inside the machine (no spaces or ':'), like /var/lib/postgresql/data"),
            );
        } else if !paths.insert(path.trim_end_matches('/')) {
            p.add(vat, format!("`{path}` is already a volume of this machine"));
        }
    }
}

/// What is wrong with volume `v`'s name on machine `machine`, if anything.
fn name_problem(machine: &str, v: &str) -> Option<&'static str> {
    if !kebab(v) {
        Some("volume names are kebab-case: lowercase letters, digits and dashes")
    } else if machine.len() + 1 + v.len() > 63 {
        // The volume's Kubernetes claim is named `<machine>-<volume>`, a 63-character label.
        Some("machine name and volume name together are at most 62 characters: the Kubernetes claim is `<machine>-<volume>`")
    } else if v.len() > 58 {
        // Its Kubernetes init container is `seed-<volume>`, a 63-character label too.
        Some("volume names are at most 58 characters: the Kubernetes init container seeding it is `seed-<volume>`")
    } else {
        None
    }
}
