//! Vagrant's providers, and what each can't give an environment's VMs: one row per provider
//! ([`Profile`]), so every per-provider rule (its shared folder, its TPM, its disks, its nested
//! virtualization switch, its console, how it boots, its bridges) is a field of that row, and a
//! new provider is a new variant whose row the compiler asks for. A provider that can't run the
//! VMs as the spec asks says so here (`provider_refusal`) instead of ignoring it.

use isoloom_manifest::{HostOs, Provider as ManifestProvider};
use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::model::Arch;

use super::{boot, bridged, hyperv, qemu, windows};

/// A Vagrant provider Isoloom knows, as `vagrant up --provider` names it ([`Profile::id`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Provider {
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
}

/// How a provider shares a folder of the computer that runs the environment both ways (Vagrant's
/// synced folder `type:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Share {
    /// Read-write and read-only, by its synced folder `type`; `ro_option` is the keyword its
    /// mount takes `ro` under.
    Both {
        /// The synced folder's `type:`.
        kind: &'static str,
        /// The keyword a read-only mount's `ro` goes under (`mount_options`, libvirt's `mount_opts`).
        ro_option: &'static str,
    },
    /// Read-write only: a read-only folder is refused, `why`.
    ReadWrite {
        /// The synced folder's `type:`.
        kind: &'static str,
        /// Why a read-only one is refused.
        why: &'static str,
    },
    /// None Isoloom uses, `why`.
    None(&'static str),
}

/// How a provider switches nested virtualization on in its block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Nested {
    /// `v.customize [<command>, :id, <flag>, "on"]` (VirtualBox, Parallels).
    Customize(&'static str, &'static str),
    /// `v.vmx["vhv.enable"] = "TRUE"` (VMware Workstation and Fusion).
    Vmx,
    /// `v.guest_custom_vmx_settings = [["vhv.enable", "TRUE"]]` (ESXi).
    CustomVmx,
    /// `v.<field> = true`.
    Flag(&'static str),
    /// No switch: the provider can't nest, and `vagrant up` stops on it
    /// ([`super::nested::refuse_elsewhere`]), or it has no block.
    None,
}

/// How a provider boots a machine that boots its own way (`hardware.boot`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OwnBoot {
    /// Every field (libvirt); UEFI needs the firmware file.
    Own,
    /// Only the firmware's format (QEMU boots the box's disk through its own firmware).
    FirmwareFormat,
    /// The box as it is.
    BoxAsIs,
}

/// Everything Isoloom knows of one provider.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Profile {
    /// The provider.
    pub(crate) provider: Provider,
    /// The name Vagrant knows it by (`vagrant up --provider <id>`, `config.vm.provider`).
    pub(crate) id: &'static str,
    /// Its name in a refusal's prose (`Hyper-V`).
    pub(crate) label: &'static str,
    /// Its id in the run manifest.
    pub(crate) manifest: ManifestProvider,
    /// The hosts it exists on.
    pub(crate) hosts: &'static [HostOs],
    /// Why the `vagrant` target has no block for it, when it has none.
    pub(crate) no_vms: Option<&'static str>,
    /// Its two-way shared folder (`host_folders`).
    pub(crate) share: Share,
    /// Whether it gives a VM a virtual TPM 2.0.
    pub(crate) tpm: bool,
    /// Whether its plugin attaches a VM's data disks.
    pub(crate) attaches_disks: bool,
    /// Whether Isoloom bridges a network under it; `false` refuses a bridged network.
    pub(crate) bridges: bool,
    /// What else it refuses of the environment's networks and boxes (QEMU's links, Hyper-V's
    /// boxes).
    pub(crate) networks: Option<fn(&Environment) -> Option<String>>,
    /// How it boots a machine that boots its own way.
    pub(crate) boot: OwnBoot,
    /// The architecture of the host its VMs run on, when that isn't the computer Vagrant runs on
    /// (ESXi), whose architecture Isoloom doesn't know when it writes the Vagrantfile.
    pub(crate) remote_arch: Option<Arch>,
    /// Its nested virtualization switch.
    pub(crate) nested: Nested,
    /// Why it gives a VM no graphical console Isoloom offers, when it gives none.
    pub(crate) no_console: Option<&'static str>,
}

const EVERY_HOST: &[HostOs] = &[HostOs::Linux, HostOs::Macos, HostOs::Windows];

/// The architecture of an ESXi host, which runs the VMs of `vmware_esxi` (not the computer
/// Vagrant runs on): ESXi is x86-64 (ESXi-Arm is a rare, separate build Isoloom doesn't target).
/// Vagrant picks the boxes' builds for it ([`crate::manifest`]'s `VAGRANT_HOST_ARCHITECTURE`).
pub(crate) const ESXI_ARCH: Arch = Arch::Amd64;

/// QEMU's links (one network per VM) and bridges.
fn qemu_networks(env: &Environment) -> Option<String> {
    qemu::refusal(super::qemu_links(env))
}

/// The defaults of a row: a desktop hypervisor on this computer that shares, nests, attaches
/// disks and gives a console, and bridges.
const DESKTOP: Profile = Profile {
    provider: Provider::VirtualBox,
    id: "",
    label: "",
    manifest: ManifestProvider::Virtualbox,
    hosts: EVERY_HOST,
    no_vms: None,
    share: Share::None(""),
    tpm: false,
    attaches_disks: true,
    bridges: true,
    networks: None,
    boot: OwnBoot::BoxAsIs,
    remote_arch: None,
    nested: Nested::None,
    no_console: None,
};

impl Provider {
    /// Every provider the Vagrantfile has a block for, in the order a runner offers them (the
    /// run manifest's) and the Vagrantfile writes their shared folders.
    pub(crate) const ALL: [Provider; 8] = [
        Provider::VirtualBox,
        Provider::VmwareDesktop,
        Provider::HyperV,
        Provider::Parallels,
        Provider::Libvirt,
        Provider::Qemu,
        Provider::Utm,
        Provider::VmwareEsxi,
    ];

    /// The provider Vagrant knows by `id`; `None` for one Isoloom doesn't know.
    pub(crate) fn parse(id: &str) -> Option<Provider> {
        Provider::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The name Vagrant knows it by (`vagrant up --provider <id>`, `config.vm.provider`).
    pub(crate) fn id(self) -> &'static str {
        self.profile().id
    }

    /// Its row: everything Isoloom knows of it.
    #[expect(clippy::too_many_lines, reason = "one table: a row per provider")]
    pub(crate) const fn profile(self) -> Profile {
        match self {
            Provider::VirtualBox => Profile {
                provider: self,
                id: "virtualbox",
                label: "VirtualBox",
                manifest: ManifestProvider::Virtualbox,
                share: Share::Both {
                    kind: "virtualbox",
                    ro_option: "mount_options",
                },
                tpm: true,
                nested: Nested::Customize("modifyvm", "--nested-hw-virt"),
                ..DESKTOP
            },
            Provider::VmwareDesktop => Profile {
                provider: self,
                id: "vmware_desktop",
                label: "VMware Desktop",
                manifest: ManifestProvider::VmwareDesktop,
                share: Share::Both {
                    kind: "vmware",
                    ro_option: "mount_options",
                },
                nested: Nested::Vmx,
                ..DESKTOP
            },
            Provider::HyperV => Profile {
                provider: self,
                id: "hyperv",
                label: "Hyper-V",
                manifest: ManifestProvider::Hyperv,
                hosts: &[HostOs::Windows],
                share: Share::None(
                    "the `hyperv` provider's only two-way shared folder is SMB, which asks for a Windows account's password at every `vagrant up`",
                ),
                tpm: true,
                bridges: false,
                networks: Some(hyperv::refusal),
                nested: Nested::Flag("enable_virtualization_extensions"),
                no_console: Some(
                    "Hyper-V has no VNC or remote display server of its own; a Windows machine's own Remote Desktop is the way, when the spec opens its port",
                ),
                ..DESKTOP
            },
            Provider::Parallels => Profile {
                provider: self,
                id: "parallels",
                label: "Parallels",
                manifest: ManifestProvider::Parallels,
                hosts: &[HostOs::Macos],
                share: Share::Both {
                    kind: "parallels",
                    ro_option: "mount_options",
                },
                attaches_disks: false,
                nested: Nested::Customize("set", "--nested-virt"),
                no_console: Some("Isoloom sets up no graphical console on Parallels Desktop (its VNC server stays off)"),
                ..DESKTOP
            },
            Provider::Libvirt => Profile {
                provider: self,
                id: "libvirt",
                label: "libvirt",
                manifest: ManifestProvider::Libvirt,
                hosts: &[HostOs::Linux],
                share: Share::Both {
                    kind: "virtiofs",
                    ro_option: "mount_opts",
                },
                tpm: true,
                bridges: false,
                boot: OwnBoot::Own,
                nested: Nested::Flag("nested"),
                ..DESKTOP
            },
            Provider::Qemu => Profile {
                provider: self,
                id: "qemu",
                label: "QEMU",
                manifest: ManifestProvider::Qemu,
                hosts: &[HostOs::Linux, HostOs::Macos],
                share: Share::ReadWrite {
                    kind: "qemu_virtiofs",
                    why: "one is read-only (`read_only`), and the `qemu` provider (vagrant-qemu) mounts its VirtioFS shares read-write",
                },
                attaches_disks: false,
                bridges: false,
                networks: Some(qemu_networks),
                boot: OwnBoot::FirmwareFormat,
                ..DESKTOP
            },
            Provider::Utm => Profile {
                provider: self,
                id: "utm",
                label: "UTM",
                manifest: ManifestProvider::Utm,
                hosts: &[HostOs::Macos],
                // `vagrant_utm` (0.1.6) gives a VM no private network, only forwarded ports on its
                // shared network, and every machine of a spec is on a network (validation asks
                // it), so UTM could run no environment's VMs.
                no_vms: Some(
                    "the `vagrant` target has no `utm` provider: vagrant_utm gives a VM no private network (it only forwards ports), and every machine is on one",
                ),
                share: Share::Both {
                    kind: "utm",
                    ro_option: "mount_options",
                },
                no_console: Some("UTM shows a VM's screen over SPICE only, which Isoloom doesn't relay"),
                ..DESKTOP
            },
            Provider::VmwareEsxi => Profile {
                provider: self,
                id: "vmware_esxi",
                label: "ESXi",
                manifest: ManifestProvider::VmwareEsxi,
                share: Share::None("the `vmware_esxi` provider's VMs run on the ESXi host, not on this computer"),
                remote_arch: Some(ESXI_ARCH),
                nested: Nested::CustomVmx,
                no_console: Some(isoloom_render::esxi::NO_CONSOLE),
                ..DESKTOP
            },
        }
    }

    /// Every provider's row, in [`Provider::ALL`]'s order.
    pub(crate) fn profiles() -> impl Iterator<Item = Profile> {
        Provider::ALL.into_iter().map(Provider::profile)
    }

    /// Whether it gives a VM a virtual TPM 2.0.
    pub(crate) fn has_tpm(self) -> bool {
        self.profile().tpm
    }

    /// Whether the `vagrant` target has a block for it: every provider but UTM.
    pub(crate) fn runs_vms(self) -> bool {
        self.profile().no_vms.is_none()
    }
}

/// Why the VMs of `env`, planned for the `vagrant` target, can't all run under Vagrant's
/// `provider` (as `vagrant up --provider` names it), if they can't: a provider the target has no
/// block for (UTM, which gives a VM no private network), a machine requiring a TPM the provider doesn't give, then QEMU's
/// networks, Hyper-V's boxes and libvirt's bridged networks, then a machine's own boot, which
/// only libvirt (and QEMU, for UEFI's format) boots, then a machine's host folders, which Hyper-V
/// and ESXi don't share (nor QEMU read-only), then a machine of another architecture than ESXi's
/// on ESXi, then a machine's data disks, which QEMU's and Parallels' plugins don't attach.
/// A spec that can't be planned at all (a machine's configuration that doesn't resolve) is
/// refused with why.
#[must_use]
pub fn provider_refusal(env: &Environment, provider: &str) -> Option<String> {
    // A provider Isoloom doesn't know has no block to refuse anything in.
    let provider = Provider::parse(provider)?;
    if let Some(why) = provider.profile().no_vms {
        return Some(why.to_owned());
    }
    windows::tpm_refusal(env, provider)
        .or_else(|| networks_refusal(env, provider))
        .or_else(|| boot::refusal(env, provider))
        .or_else(|| crate::host_folders::refusal(env, provider))
        .or_else(|| arch_refusal(env, provider))
        .or_else(|| disk_refusal(env, provider))
}

/// A VM ESXi can't run: one of another architecture than the ESXi host's ([`ESXI_ARCH`]).
/// Every other provider runs its VMs on the computer Vagrant runs on, whose architecture
/// Isoloom doesn't know when it writes the Vagrantfile.
fn arch_refusal(env: &Environment, provider: Provider) -> Option<String> {
    let arch = provider.profile().remote_arch?;
    let (name, mp) = env.machines.iter().find(|(_, mp)| mp.shape == MachineShape::Vm && mp.arch != arch)?;
    Some(format!(
        "machine `{name}` is {} (`hardware.arch`), and {} runs {} VMs only",
        mp.arch.id(),
        provider.profile().label,
        match arch {
            Arch::Amd64 => "x86-64",
            Arch::Arm64 => "arm64",
        }
    ))
}

/// Why `provider` can't give this environment's VMs their data disks, if it can't: QEMU's and
/// Parallels' plugins attach none.
fn disk_refusal(env: &Environment, provider: Provider) -> Option<String> {
    if provider.profile().attaches_disks {
        return None;
    }
    let (name, _) = env.machines.iter().find(|(_, mp)| mp.shape == MachineShape::Vm && !mp.disks.is_empty())?;
    Some(format!(
        "machine `{name}` has data disks (`disks`), which the `{}` provider doesn't attach",
        provider.id()
    ))
}

/// What `provider` can't give the VMs of their networks and boxes: QEMU links a VM on one
/// network only, Hyper-V needs a Hyper-V build of each box, and Isoloom bridges no network on
/// QEMU, Hyper-V or libvirt.
fn networks_refusal(env: &Environment, provider: Provider) -> Option<String> {
    let profile = provider.profile();
    profile
        .networks
        .and_then(|refusal| refusal(env))
        .or_else(|| (!profile.bridges).then(|| bridged::provider_refusal(env, profile.label)).flatten())
}

#[cfg(test)]
mod tests {
    use super::Provider;

    #[test]
    fn a_provider_reads_back_as_vagrant_names_it_and_has_its_own_row() {
        for p in Provider::ALL {
            assert_eq!(p.profile().provider, p);
            assert_eq!(Provider::parse(p.id()), Some(p));
        }
        assert_eq!(Provider::parse("docker"), None);
        let ids: std::collections::BTreeSet<&str> = Provider::ALL.iter().map(|p| p.id()).collect();
        assert_eq!(ids.len(), Provider::ALL.len());
    }

    #[test]
    fn only_virtualbox_libvirt_and_hyperv_give_a_tpm() {
        let with: Vec<&str> = Provider::ALL.into_iter().filter(|p| p.has_tpm()).map(Provider::id).collect();
        assert_eq!(with, ["virtualbox", "hyperv", "libvirt"]);
    }
}
