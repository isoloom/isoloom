//! `overrides:` lowered into the fields today's model keeps them in: a Docker block, an
//! existing network or volume (the target being lowered's own only), a machine's VM images (one
//! per target, so all of them at once), an LXC container on Proxmox, an existing host.

use indexmap::IndexMap;
use isoloom_spec::model::{External, Network, NetworkDocker, SharedVolume, Target, VmImage};
use isoloom_spec::syntax::{self, FieldPath};

use super::Scope;

/// The networks: each with its Docker block, and the existing network the target being
/// lowered joins instead (a Docker network, a Proxmox bridge, an ESXi port group).
pub(super) fn networks(source: &syntax::Spec, scope: Scope) -> IndexMap<String, Network> {
    let o = &source.overrides;
    let docker = o.docker.as_ref();
    let mut out = source.networks.clone();
    for (name, n) in &mut out {
        if let Some(cidr) = docker.and_then(|d| d.networks.get(name)).and_then(|d| d.cidr.clone()) {
            n.docker = Some(NetworkDocker { cidr });
        }
        n.external = match scope.target() {
            Some(t) if docker_target(t) => docker.and_then(|d| d.networks.get(name)).and_then(|d| d.external.clone()),
            Some(Target::Proxmox) => o.proxmox.as_ref().and_then(|p| p.networks.get(name)).map(|e| e.external.clone()),
            Some(Target::Esxi) => o.esxi.as_ref().and_then(|p| p.networks.get(name)).map(|e| e.external.clone()),
            _ => None,
        };
    }
    out
}

/// Whether the target runs the Compose file (`overrides.docker`).
fn docker_target(t: Target) -> bool {
    syntax::Overrides::targets_of("docker").contains(&t)
}

/// The shared volumes: each existing one on the target being lowered, and Proxmox's virtiofs
/// (only Proxmox reads it).
pub(super) fn volumes(source: &syntax::Spec, scope: Scope) -> IndexMap<String, SharedVolume> {
    let o = &source.overrides;
    let mut out = source.volumes.clone();
    for (name, v) in &mut out {
        v.virtiofs = o.proxmox.as_ref().and_then(|p| p.volumes.get(name)).map(|p| p.virtiofs.clone());
        v.external = match scope.target() {
            Some(t) if docker_target(t) => o.docker.as_ref().and_then(|d| d.volumes.get(name)).map(|e| e.external.clone()),
            Some(Target::Kubernetes) => o.kubernetes.as_ref().and_then(|k| k.volumes.get(name)).map(|e| e.external.clone()),
            _ => None,
        };
    }
    out
}

/// A value read from the spec, with the path it is written at: what reports where it came from
/// takes `at`, never a path rebuilt from the machine it was looked up for.
#[derive(Debug)]
pub(super) struct Found<'a, T> {
    /// The value.
    pub value: &'a T,
    /// Where it is written (`overrides.vagrant.machines.windows` for a group's entry).
    pub at: FieldPath,
}

/// The entry of `overrides.<section>.machines` (`entries`) for machine `name`: its own, else a
/// counted machine's (its clones share it), else the first of its groups'.
pub(super) fn for_machine<'a, T>(source: &syntax::Spec, section: &str, entries: &'a IndexMap<String, T>, name: &str) -> Option<Found<'a, T>> {
    let clone_base = || {
        source
            .clones
            .iter()
            .find(|(_, c)| c.iter().any(|n| n == name))
            .and_then(|(base, _)| entries.get_key_value(base.as_str()))
    };
    let (key, value) = entries
        .get_key_value(name)
        .or_else(clone_base)
        .or_else(|| entries.iter().find(|(key, _)| member(source, key, name, 0)))?;
    let at = FieldPath::new("overrides").key(section).key("machines").key(key);
    Some(Found { value, at })
}

/// Whether group `group` holds machine `name` (its members' names, globs and nested groups).
fn member(source: &syntax::Spec, group: &str, name: &str, depth: usize) -> bool {
    let Some(g) = source.groups.get(group).filter(|_| depth < 8) else {
        return false;
    };
    g.members
        .iter()
        .any(|p| isoloom_spec::groups::matches(p, name) || (p != group && member(source, p, name, depth + 1)))
}

/// Whether machine `name` runs as an LXC container on Proxmox (lowered for Proxmox only).
pub(super) fn lxc(source: &syntax::Spec, name: &str, scope: Scope) -> bool {
    scope == Scope::Target(Target::Proxmox)
        && source
            .overrides
            .proxmox
            .as_ref()
            .and_then(|p| for_machine(source, "proxmox", &p.machines, name))
            .is_some_and(|m| m.value.lxc)
}

/// Machine `name`'s existing host, from `overrides.external`.
pub(super) fn external(source: &syntax::Spec, name: &str) -> Option<External> {
    let x = source.overrides.external.as_ref()?;
    for_machine(source, "external", &x.machines, name).map(|m| m.value.ssh.clone())
}

/// Machine `name`'s VM images, one per target: its Vagrant box, its Proxmox image, each cloud's.
pub(super) fn vm_image(source: &syntax::Spec, name: &str) -> Option<VmImage> {
    let o = &source.overrides;
    let mut image = VmImage::default();
    if let Some(Found { value: v, .. }) = o.vagrant.as_ref().and_then(|v| for_machine(source, "vagrant", &v.machines, name)) {
        image.vagrant.clone_from(&v.box_name);
        image.vagrant_version.clone_from(&v.box_version);
        image.box_url.clone_from(&v.box_url);
        image.box_server_url.clone_from(&v.box_server_url);
        image.qemu = v.boxes.as_ref().and_then(|b| b.qemu.clone());
        image.hyperv = v.boxes.as_ref().and_then(|b| b.hyperv.clone());
        image.utm = v.boxes.as_ref().and_then(|b| b.utm.clone());
        image.winrm = v.winrm;
        image.winssh = v.winssh;
    }
    image.proxmox = o
        .proxmox
        .as_ref()
        .and_then(|p| for_machine(source, "proxmox", &p.machines, name))
        .and_then(|m| m.value.image.clone());
    for (cloud, c) in o.clouds() {
        let id = c.and_then(|c| for_machine(source, cloud, &c.machines, name)).map(|m| m.value.image.clone());
        match cloud {
            "aws" => image.aws = id,
            "azure" => image.azure = id,
            "gcp" => image.gcp = id,
            "digitalocean" => image.digitalocean = id,
            "linode" => image.linode = id,
            _ => image.oci = id,
        }
    }
    (image != VmImage::default()).then_some(image)
}

#[cfg(test)]
mod tests;
