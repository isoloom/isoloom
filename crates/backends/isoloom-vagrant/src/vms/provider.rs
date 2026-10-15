//! Vagrant's providers, and what each can't give an environment's VMs. Every provider's block
//! is in one Vagrantfile, so a provider that can't run the VMs as the spec asks says so here
//! (`refusal_on` with the provider) instead of ignoring it.

use isoloom_plan::Environment;
use isoloom_spec::model::{Spec, Target};

use super::{boot, bridged, hyperv, qemu, windows};

/// A Vagrant provider, as `vagrant up --provider` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Provider<'a> {
    /// VirtualBox.
    VirtualBox,
    /// VMware Workstation and Fusion.
    VmwareDesktop,
    /// An ESXi host (vagrant-vmware-esxi).
    VmwareEsxi,
    /// Parallels Desktop.
    Parallels,
    /// UTM, on a Mac (`vagrant_utm`).
    Utm,
    /// QEMU (vagrant-qemu).
    Qemu,
    /// libvirt (vagrant-libvirt).
    Libvirt,
    /// Hyper-V.
    HyperV,
    /// One Isoloom doesn't know, by the name it was given.
    Other(&'a str),
}

impl<'a> From<&'a str> for Provider<'a> {
    fn from(name: &'a str) -> Provider<'a> {
        match name {
            "virtualbox" => Provider::VirtualBox,
            "vmware_desktop" => Provider::VmwareDesktop,
            "vmware_esxi" => Provider::VmwareEsxi,
            "parallels" => Provider::Parallels,
            "utm" => Provider::Utm,
            "qemu" => Provider::Qemu,
            "libvirt" => Provider::Libvirt,
            "hyperv" => Provider::HyperV,
            other => Provider::Other(other),
        }
    }
}

impl<'a> Provider<'a> {
    /// The name Vagrant knows it by (`vagrant up --provider <id>`, `config.vm.provider`).
    pub(crate) fn id(self) -> &'a str {
        match self {
            Provider::VirtualBox => "virtualbox",
            Provider::VmwareDesktop => "vmware_desktop",
            Provider::VmwareEsxi => "vmware_esxi",
            Provider::Parallels => "parallels",
            Provider::Utm => "utm",
            Provider::Qemu => "qemu",
            Provider::Libvirt => "libvirt",
            Provider::HyperV => "hyperv",
            Provider::Other(name) => name,
        }
    }

    /// Whether it gives a VM a virtual TPM 2.0.
    pub(crate) fn has_tpm(self) -> bool {
        matches!(self, Provider::VirtualBox | Provider::Libvirt | Provider::HyperV)
    }

    /// Whether its plugin attaches a VM's data disks.
    fn attaches_disks(self) -> bool {
        !matches!(self, Provider::Qemu | Provider::Parallels | Provider::Utm)
    }
}

/// Why this environment's VMs can't all run under Vagrant's `provider` (as `vagrant up
/// --provider` names it), if they can't: a machine requiring a TPM the provider doesn't give,
/// then QEMU's and UTM's networks, Hyper-V's boxes and libvirt's bridged networks, then a
/// machine's own boot, which only libvirt (and QEMU, for UEFI's format) boots.
#[must_use]
pub fn provider_refusal(spec: &Spec, provider: &str) -> Option<String> {
    let env = Environment::build(spec, Target::Vagrant, None);
    let provider = Provider::from(provider);
    windows::tpm_refusal(&env, provider)
        .or_else(|| networks_refusal(&env, provider))
        .or_else(|| boot::refusal(&env, provider))
}

/// Why `provider` can't give this environment's VMs their data disks, if it can't: QEMU's,
/// Parallels' and UTM's plugins attach none.
#[must_use]
pub fn disk_refusal(spec: &Spec, provider: &str) -> Option<String> {
    let provider = Provider::from(provider);
    if provider.attaches_disks() {
        return None;
    }
    let (name, _) = spec.machines.iter().find(|(_, m)| m.vm.is_some() && !m.disks.is_empty())?;
    Some(format!(
        "machine `{name}` has data disks (`disks`), which the `{}` provider doesn't attach",
        provider.id()
    ))
}

/// What `provider` can't give the VMs of their networks and boxes: QEMU links a VM on one
/// network only, `vagrant_utm` (0.1.6) sets up no private network (it only forwards ports to
/// the VM's shared network, so a VM on one of the spec's networks would silently have none),
/// Hyper-V needs a Hyper-V build of each box, and QEMU, Hyper-V and libvirt have no bridged
/// networks yet.
fn networks_refusal(env: &Environment, provider: Provider<'_>) -> Option<String> {
    match provider {
        Provider::Qemu => qemu::refusal(super::qemu_links(env)).or_else(|| bridged::provider_refusal(env, "QEMU")),
        Provider::Utm => {
            let (name, _) = super::qemu_links(env).into_iter().find(|(_, l)| *l != qemu::Link::None)?;
            Some(format!(
                "`{name}` is on a network, and vagrant_utm gives a VM no private network (it only forwards ports)"
            ))
        }
        Provider::HyperV => hyperv::refusal(env).or_else(|| bridged::provider_refusal(env, "Hyper-V")),
        Provider::Libvirt => bridged::provider_refusal(env, "libvirt"),
        Provider::VirtualBox | Provider::VmwareDesktop | Provider::VmwareEsxi | Provider::Parallels | Provider::Other(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::Provider;

    #[test]
    fn a_provider_reads_back_as_vagrant_names_it() {
        for id in [
            "virtualbox",
            "vmware_desktop",
            "vmware_esxi",
            "parallels",
            "utm",
            "qemu",
            "libvirt",
            "hyperv",
            "docker",
        ] {
            assert_eq!(Provider::from(id).id(), id);
        }
        assert_eq!(Provider::from("docker"), Provider::Other("docker"));
    }

    #[test]
    fn only_virtualbox_libvirt_and_hyperv_give_a_tpm() {
        let with: Vec<&str> = ["virtualbox", "vmware_desktop", "parallels", "utm", "qemu", "libvirt", "hyperv"]
            .into_iter()
            .filter(|id| Provider::from(*id).has_tpm())
            .collect();
        assert_eq!(with, ["virtualbox", "libvirt", "hyperv"]);
    }
}
