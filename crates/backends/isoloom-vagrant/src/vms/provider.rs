//! Vagrant's providers, and what each can't give an environment's VMs. Every provider's block
//! is in one Vagrantfile, so a provider that can't run the VMs as the spec asks says so here
//! (`refusal_on` with the provider) instead of ignoring it.

use isoloom_manifest::{HostOs, Provider as ManifestProvider};
use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::model::{Arch, Spec, Target};

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

/// What Isoloom knows of one provider beyond its name: one row of [`Provider::ALL`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Known {
    /// The provider.
    pub(crate) provider: Provider<'static>,
    /// Its id in the run manifest.
    pub(crate) manifest: ManifestProvider,
    /// How it shares a folder of the computer that runs the lab both ways (Vagrant's synced
    /// folder `type:`), if it has a way Isoloom uses (`host_folders`).
    pub(crate) share: Option<&'static str>,
    /// The hosts it exists on.
    pub(crate) hosts: &'static [HostOs],
}

const EVERY_HOST: &[HostOs] = &[HostOs::Linux, HostOs::Macos, HostOs::Windows];

/// The architecture of an ESXi host, which runs the VMs of `vmware_esxi` (not the computer
/// Vagrant runs on): ESXi is x86-64 (ESXi-Arm is a rare, separate build Isoloom doesn't target).
/// Vagrant picks the boxes' builds for it ([`crate::manifest`]'s `VAGRANT_HOST_ARCHITECTURE`).
pub(crate) const ESXI_ARCH: Arch = Arch::Amd64;

impl Provider<'static> {
    /// Every provider the Vagrantfile has a block for, in the order a runner offers them (the
    /// run manifest's) and the Vagrantfile writes their shared folders.
    pub(crate) const ALL: [Known; 8] = [
        Known {
            provider: Provider::VirtualBox,
            manifest: ManifestProvider::Virtualbox,
            share: Some("virtualbox"),
            hosts: EVERY_HOST,
        },
        Known {
            provider: Provider::VmwareDesktop,
            manifest: ManifestProvider::VmwareDesktop,
            share: Some("vmware"),
            hosts: EVERY_HOST,
        },
        // Its only two-way shared folder is SMB, which asks for a password at every `vagrant up`.
        Known {
            provider: Provider::HyperV,
            manifest: ManifestProvider::Hyperv,
            share: None,
            hosts: &[HostOs::Windows],
        },
        Known {
            provider: Provider::Parallels,
            manifest: ManifestProvider::Parallels,
            share: Some("parallels"),
            hosts: &[HostOs::Macos],
        },
        Known {
            provider: Provider::Libvirt,
            manifest: ManifestProvider::Libvirt,
            share: Some("virtiofs"),
            hosts: &[HostOs::Linux],
        },
        Known {
            provider: Provider::Qemu,
            manifest: ManifestProvider::Qemu,
            share: Some("qemu_virtiofs"),
            hosts: &[HostOs::Linux, HostOs::Macos],
        },
        Known {
            provider: Provider::Utm,
            manifest: ManifestProvider::Utm,
            share: Some("utm"),
            hosts: &[HostOs::Macos],
        },
        // Its VMs run on the ESXi host, not on the computer that runs the lab: nothing to share.
        Known {
            provider: Provider::VmwareEsxi,
            manifest: ManifestProvider::VmwareEsxi,
            share: None,
            hosts: EVERY_HOST,
        },
    ];
}

impl<'a> From<&'a str> for Provider<'a> {
    fn from(name: &'a str) -> Provider<'a> {
        Provider::ALL
            .into_iter()
            .map(|k| k.provider)
            .find(|p| p.id() == name)
            .unwrap_or(Provider::Other(name))
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

    /// Its row of [`Provider::ALL`]; `None` for one Isoloom doesn't know.
    pub(crate) fn known(self) -> Option<Known> {
        Provider::ALL.into_iter().find(|k| k.provider == self)
    }

    /// Whether it gives a VM a virtual TPM 2.0.
    pub(crate) fn has_tpm(self) -> bool {
        matches!(self, Provider::VirtualBox | Provider::Libvirt | Provider::HyperV)
    }

    /// Whether its plugin attaches a VM's data disks.
    fn attaches_disks(self) -> bool {
        !matches!(self, Provider::Qemu | Provider::Parallels)
    }

    /// Whether the `vagrant` target has a block for it: every provider but UTM. `vagrant_utm`
    /// (0.1.6) gives a VM no private network, only forwarded ports on its shared network, and
    /// every machine of a spec is on a network (validation asks it), so UTM could run no
    /// environment's VMs; it runs `docker-vm`'s one VM, which needs no private network.
    pub(crate) fn runs_vms(self) -> bool {
        self != Provider::Utm
    }
}

/// Why this environment's VMs can't all run under Vagrant's `provider` (as `vagrant up
/// --provider` names it), if they can't: a provider the target has no block for (UTM,
/// [`Provider::runs_vms`]), a machine requiring a TPM the provider doesn't give, then QEMU's
/// networks, Hyper-V's boxes and libvirt's bridged networks, then a machine's own boot, which
/// only libvirt (and QEMU, for UEFI's format) boots, then a machine's host folders, which Hyper-V
/// and ESXi don't share (nor QEMU read-only), then a machine of another architecture than ESXi's
/// on ESXi, then a machine's data disks, which QEMU's and Parallels' plugins don't attach.
#[must_use]
pub fn provider_refusal(spec: &Spec, provider: &str) -> Option<String> {
    let env = Environment::build(spec, Target::Vagrant, None);
    let provider = Provider::from(provider);
    if !provider.runs_vms() {
        return Some(format!(
            "the `vagrant` target has no `{}` provider: vagrant_utm gives a VM no private network (it only forwards ports), and every machine is on one; UTM runs `docker-vm`",
            provider.id()
        ));
    }
    windows::tpm_refusal(&env, provider)
        .or_else(|| networks_refusal(&env, provider))
        .or_else(|| boot::refusal(&env, provider))
        .or_else(|| crate::host_folders::refusal(&env, provider))
        .or_else(|| arch_refusal(&env, provider))
        .or_else(|| disk_refusal(spec, provider))
}

/// A VM ESXi can't run: one of another architecture than the ESXi host's ([`ESXI_ARCH`]).
/// Every other provider runs its VMs on the computer Vagrant runs on, whose architecture
/// Isoloom doesn't know when it writes the Vagrantfile.
fn arch_refusal(env: &Environment, provider: Provider<'_>) -> Option<String> {
    if provider != Provider::VmwareEsxi {
        return None;
    }
    let (name, mp) = env.machines.iter().find(|(_, mp)| mp.shape == MachineShape::Vm && mp.arch != ESXI_ARCH)?;
    Some(format!("machine `{name}` is {} (`hardware.arch`), and ESXi runs x86-64 VMs only", mp.arch.id()))
}

/// Why `provider` can't give this environment's VMs their data disks, if it can't: QEMU's and
/// Parallels' plugins attach none.
fn disk_refusal(spec: &Spec, provider: Provider<'_>) -> Option<String> {
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
/// network only, Hyper-V needs a Hyper-V build of each box, and Isoloom bridges no network on
/// QEMU, Hyper-V or libvirt.
fn networks_refusal(env: &Environment, provider: Provider<'_>) -> Option<String> {
    match provider {
        Provider::Qemu => qemu::refusal(super::qemu_links(env)).or_else(|| bridged::provider_refusal(env, "QEMU")),
        Provider::HyperV => hyperv::refusal(env).or_else(|| bridged::provider_refusal(env, "Hyper-V")),
        Provider::Libvirt => bridged::provider_refusal(env, "libvirt"),
        Provider::VirtualBox | Provider::VmwareDesktop | Provider::VmwareEsxi | Provider::Parallels | Provider::Utm | Provider::Other(_) => None,
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
        assert!(Provider::Other("docker").known().is_none());
        assert!(Provider::ALL.iter().all(|k| k.provider.known().is_some()));
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
