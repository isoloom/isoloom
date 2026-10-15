//! Volumes: a machine's own (names Kubernetes can carry, safe mount paths) and the ones declared
//! at the top level (`volumes:`): shared between machines, kept past `down`, or existing ones.

use std::collections::BTreeSet;

use super::machines::Subject;
use super::{Problems, kebab};
use crate::model::{SharedVolume, Spec};

/// The volumes shared between machines: mounted, held by one of their users, reachable from
/// every other over a network they share (NFS on the VM targets), Linux machines only.
pub(super) fn shared(spec: &Spec, p: &mut Problems) {
    for (v, volume) in &spec.volumes {
        let at = format!("volumes.{v}");
        let users = spec.volume_users(v);
        if users.is_empty() {
            p.add(at, "no machine mounts it: list it in a machine's `volumes`");
            continue;
        }
        if let Some(o) = &volume.owner
            && !users.contains(&o.as_str())
        {
            p.add(
                format!("{at}.owner"),
                format!("`{o}` doesn't mount it: the owner is one of its machines ({})", users.join(", ")),
            );
            continue;
        }
        kept(volume, &at, p);
        host_folder(volume, &at, p);
        let Some(owner) = spec.volume_owner(v) else { continue };
        let owner_networks = spec.machines.get(owner).map(|o| &o.networks);
        for (u, m) in users.iter().filter_map(|u| Some((*u, spec.machines.get(*u)?))) {
            if m.vm.as_ref().is_some_and(|vm| crate::images::is_windows(&vm.os)) {
                p.add(format!("machines.{u}.volumes.{v}"), windows_why(volume));
            }
            if u != owner && !m.networks.keys().any(|n| owner_networks.is_some_and(|o| o.contains_key(n))) {
                p.add(
                    format!("machines.{u}.volumes.{v}"),
                    format!("`{u}` shares no network with `{owner}`, which holds the volume (NFS on the VM targets)"),
                );
            }
        }
    }
}

/// Why a Windows VM can't mount shared volume `v`: NFS, and with `virtiofs` the virtio-fs
/// driver and WinFsp, which the guest set-up doesn't install.
fn windows_why(volume: &SharedVolume) -> &'static str {
    if volume.virtiofs.is_some() {
        "a shared volume is mounted over NFS, or virtiofs on Proxmox, whose Windows driver (virtio-fs and WinFsp) the set-up doesn't install: Linux machines only"
    } else {
        "a shared volume is mounted over NFS: Linux machines only"
    }
}

/// A volume shared as a folder of the Proxmox server: a mapping named as Proxmox names them.
fn host_folder(volume: &SharedVolume, at: &str, p: &mut Problems) {
    let Some(mapping) = volume.virtiofs.as_ref().and_then(|f| f.mapping.as_deref()) else {
        return;
    };
    if !crate::model::mapping_id(mapping) {
        p.add(
            format!("{at}.virtiofs.mapping"),
            format!("`{mapping}` isn't a directory mapping id: a letter, then letters, digits, '-' and '_', at most 64"),
        );
    }
}

/// A volume kept past `down` or existing already: one or the other (an existing volume is
/// never removed), and an existing one named as Docker and Kubernetes name volumes.
fn kept(vol: &SharedVolume, at: &str, p: &mut Problems) {
    let Some(name) = &vol.external else { return };
    if vol.persistent {
        p.add(format!("{at}.persistent"), "an external volume is never removed: leave `persistent` out");
    }
    let valid = name.len() <= 63
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !valid {
        p.add(
            format!("{at}.external"),
            format!("`{name}` isn't a volume name: letters, digits, '-', '_' and '.', starting with a letter or digit, at most 63"),
        );
    }
}

/// A machine's volumes: kebab-case names that fit Kubernetes' labels, distinct, safe absolute
/// paths, and a folder of a shared volume only where it can be one. Then its other storage.
pub(super) fn of_machine(s: &Subject, p: &mut Problems) {
    let mut paths = BTreeSet::new();
    for (v, mount) in &s.m.volumes {
        let vat = format!("{}.volumes.{v}", s.at);
        if let Some(why) = name_problem(s.name, v) {
            p.add(&vat, why);
        }
        let path = mount.path();
        if !safe_path(path) {
            p.add(
                &vat,
                format!("`{path}` isn't a safe absolute path inside the machine (no spaces or ':'), like /var/lib/postgresql/data"),
            );
        } else if !paths.insert(path.trim_end_matches('/')) {
            p.add(&vat, format!("`{path}` is already a volume of this machine"));
        }
        if let Some(sub) = mount.subpath() {
            subpath(s, v, sub, &format!("{vat}.subpath"), p);
        }
    }
    super::storage::of_machine(s, &paths, p);
}

/// A safe absolute path for a mount. No spaces or ':' : the Docker short mount syntax
/// (`<vol>:<path>`) and the `mkdir -p` the VM/Proxmox provisioners run both split on those, so
/// either would corrupt the mount.
pub(super) fn safe_path(path: &str) -> bool {
    path.starts_with('/') && path != "/" && !path.split('/').any(|s| s == "..") && !path.contains([':', ' ', '\t'])
}

/// A folder of a shared volume: a relative path inside it, taken by a machine other than the
/// owner (which holds the whole volume, and serves it on the VM targets).
fn subpath(s: &Subject, v: &str, sub: &str, at: &str, p: &mut Problems) {
    if !s.spec.volumes.contains_key(v) {
        p.add(
            at,
            format!("only a shared volume has folders to take: declare `{v}` under the top-level `volumes`"),
        );
    } else if s.spec.volume_owner(v) == Some(s.name) {
        p.add(
            at,
            format!(
                "`{}` holds the whole volume `{v}`: give the subpath to another machine, or make another one the `owner`",
                s.name
            ),
        );
    }
    let safe = !sub.is_empty()
        && !sub.starts_with('/')
        && sub.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
        && sub.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    if !safe {
        p.add(
            at,
            format!("`{sub}` isn't a folder inside the volume: a relative path like `www` or `logs/app`, no `..`"),
        );
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
