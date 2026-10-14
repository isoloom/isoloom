//! Vagrant: the machine settings (`config.vm.*`), network types, provisioners and pushes; the
//! options of networks, disks, synced folders, provisioners and communicators; then each
//! provider plugin's settings. The names come from the pinned upstream sources
//! (`coverage/vagrant/*.txt`, see SOURCES.md, refreshed by `scripts/refresh-vagrant.py`); every
//! one must be classified here, or the coverage tests fail.
//!
//! The tables are split by section so each stays readable: `core` for `config.vm` and the
//! other top-level blocks, one module per kind of option, and one per provider plugin.

mod communicators;
mod core;
mod docker;
mod esxi;
mod folders;
mod libvirt;
mod networks;
mod providers;
mod provisioners;
mod qemu;

use super::{
    Format, Support,
    Support::{Emitted, Equivalent, NotPortable, Planned, Tooling, Unclassified},
};

/// The Vagrant release every list from hashicorp/vagrant comes from (the plugins pin their own).
pub const VAGRANT_VERSION: &str = "2.4.9";

pub(crate) const VM: &str = include_str!("../../coverage/vagrant/vm.txt");
const VIRTUALBOX: &str = include_str!("../../coverage/vagrant/virtualbox.txt");
const HYPERV: &str = include_str!("../../coverage/vagrant/hyperv.txt");
const VMWARE: &str = include_str!("../../coverage/vagrant/vmware_desktop.txt");
const PARALLELS: &str = include_str!("../../coverage/vagrant/parallels.txt");
const LIBVIRT: &str = include_str!("../../coverage/vagrant/libvirt.txt");
const UTM: &str = include_str!("../../coverage/vagrant/utm.txt");
const QEMU: &str = include_str!("../../coverage/vagrant/qemu.txt");
const VMWARE_ESXI: &str = include_str!("../../coverage/vagrant/vmware_esxi.txt");
const DOCKER: &str = include_str!("../../coverage/vagrant/docker.txt");
pub(crate) const NETWORKS: &str = include_str!("../../coverage/vagrant/networks.txt");
pub(crate) const PROVISIONERS: &str = include_str!("../../coverage/vagrant/provisioners.txt");
pub(crate) const PUSH: &str = include_str!("../../coverage/vagrant/push.txt");
const FOLDERS: &str = include_str!("../../coverage/vagrant/folders.txt");
const COMMUNICATORS: &str = include_str!("../../coverage/vagrant/communicators.txt");

/// The setting names in one extracted list (comment lines skipped).
pub fn names(list: &str) -> Vec<&str> {
    list.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect()
}

/// The `(kind, option)` pairs of a grouped list (`shell inline`), in the list's order.
pub fn pairs(list: &str) -> Vec<(&str, &str)> {
    names(list).into_iter().filter_map(|l| l.split_once(' ')).collect()
}

/// The kinds of a grouped list (`shell`, `file`...), each once, in the list's order.
pub(crate) fn kinds(list: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for (kind, _) in pairs(list) {
        if !out.contains(&kind) {
            out.push(kind);
        }
    }
    out
}

pub(crate) const TOOLING: Support = Tooling {
    note: "Vagrant tooling, not the environment's behavior",
};
pub(crate) const USER_SETUP: Support = Tooling {
    note: "the user's own setup (how Vagrant reaches the hypervisor, where it stores things)",
};
pub(crate) const HOST_TUNING: Support = NotPortable {
    why: "hypervisor tuning: containers and cloud VMs have no such knob",
};
pub(crate) const DEVICES: Support = NotPortable {
    why: "display, input and host devices: containers and cloud VMs have none",
};
pub(crate) const PROVIDER_SPECIFIC: Support = NotPortable {
    why: "raw commands for one hypervisor: no other target understands them",
};
pub(crate) const NO_SHARED_FOLDERS: Support = Tooling {
    note: "shared folders are off: the project is copied into each VM",
};
pub(crate) const PRIVATE_NETWORKS: Support = Equivalent {
    via: "`config.vm.network private_network`, the same on every provider",
};
pub(crate) const IMAGE_BOOTS: Support = Planned {
    note: "custom images with their own kernel and boot (makes an environment VM-only)",
};
pub(crate) const LINKED_CLONE: Support = Tooling {
    note: "faster starts from one image: no change in behavior",
};
pub(crate) const LINKED_CLONE_ON: Support = Emitted {
    from: "(a differencing disk, so a clone is instant instead of copying the whole box)",
};
/// The VM's own disk, sized: a minimum (a bigger box keeps its own), the root filesystem grown.
pub(crate) const DISK: Support = Emitted {
    from: "machines.*.resources.disk_gb (the root filesystem grown into it)",
};
/// More disks: a machine's data lives on its one disk, sized by `disk_gb`, at its volumes' paths.
pub(crate) const MORE_DISKS: Support = Equivalent {
    via: "machines.*.resources.disk_gb and machines.*.volumes (one disk, sized, holding the volumes' paths)",
};
pub(crate) const ARCH: Support = Emitted { from: "machines.*.arch" };
pub(crate) const WINDOWS: Support = Planned {
    note: "with Windows guests (which make an environment VM-only)",
};
pub(crate) const RESOURCES: Support = Emitted { from: "machines.*.resources" };
pub(crate) const VM_NAME: Support = Emitted {
    from: "(the environment and machine names, as the VM's display name)",
};
pub(crate) const MAC: Support = NotPortable {
    why: "cloud VMs get their MAC and addresses from the provider",
};
pub(crate) const IPV6: Support = Planned { note: "IPv6 networks" };

/// The Vagrant tables: `config.vm` and the other blocks, their options, then one per provider.
pub fn formats() -> Vec<Format> {
    vec![
        core::format(),
        networks::format(),
        folders::format(),
        provisioners::format(),
        communicators::format(),
        provider("Vagrant: VirtualBox", "virtualbox", VIRTUALBOX, providers::virtualbox),
        provider("Vagrant: VMware Desktop", "vmware_desktop", VMWARE, providers::vmware),
        provider("Vagrant: Parallels", "parallels", PARALLELS, providers::parallels),
        provider("Vagrant: libvirt", "libvirt", LIBVIRT, libvirt::classify),
        provider("Vagrant: Hyper-V", "hyperv", HYPERV, providers::hyperv),
        utm(),
        provider("Vagrant: QEMU", "qemu", QEMU, qemu::classify),
        provider("Vagrant: ESXi", "vmware_esxi", VMWARE_ESXI, esxi::classify),
        provider("Vagrant: Docker", "docker", DOCKER, docker::classify),
    ]
}

fn utm() -> Format {
    let mut f = provider("Vagrant: UTM", "utm", UTM, providers::utm);
    // Every Isoloom network is a private network; UTM's plugin doesn't say it supports them.
    f.rows.push((
        "provider utm: private_network".to_string(),
        Planned {
            note: "not verified: vagrant_utm doesn't document `config.vm.network private_network`",
        },
    ));
    f
}

/// A table of the Vagrantfile: every name of `keys`, classified (unclassified when not).
pub(crate) fn table(name: &'static str, source: &'static str, keys: Vec<(String, Option<Support>)>) -> Format {
    Format {
        name,
        file: ".isoloom/vagrant/Vagrantfile",
        source,
        rows: keys.into_iter().map(|(k, s)| (k, s.unwrap_or(Unclassified))).collect(),
        collapse_not_portable: false,
    }
}

fn provider(name: &'static str, id: &'static str, list: &'static str, classify: fn(&str) -> Option<Support>) -> Format {
    let source = match id {
        "vmware_desktop" => "vagrant-vmware-desktop 3.0.5 (its config class)",
        "parallels" => "vagrant-parallels 2.4.7 (its config class)",
        "libvirt" => "vagrant-libvirt 0.12.2 (its config class)",
        "utm" => "vagrant_utm 0.1.6 (its config class), for Apple Silicon Macs",
        "qemu" => "vagrant-qemu 0.6.3 (its config class), for Apple Silicon Macs",
        "vmware_esxi" => "vagrant-vmware-esxi 2.5.2 (its config class), for standalone ESXi hosts",
        _ => "Vagrant 2.4.9 (the provider's config class)",
    };
    let keys = names(list).into_iter().map(|s| (format!("provider {id}: {s}"), classify(s))).collect();
    table(name, source, keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The version shown on the page is the one the lists were extracted from.
    #[test]
    fn the_lists_come_from_the_pinned_vagrant() {
        let pin = format!("/v{VAGRANT_VERSION}/");
        for list in [VM, VIRTUALBOX, HYPERV, DOCKER] {
            assert!(list.lines().next().is_some_and(|l| l.contains(&pin)), "{:?}", list.lines().next());
        }
        let pin = format!(" v{VAGRANT_VERSION}");
        for list in [NETWORKS, PROVISIONERS, PUSH, FOLDERS, COMMUNICATORS] {
            assert!(list.lines().next().is_some_and(|l| l.contains(&pin)), "{:?}", list.lines().next());
        }
        for f in formats() {
            assert!(
                !f.source.starts_with("Vagrant ") || f.source.starts_with(&format!("Vagrant {VAGRANT_VERSION} ")),
                "{}",
                f.source
            );
        }
    }
}
