//! Vagrant's providers, and what each can't give an environment's VMs: one row per provider
//! ([`Profile`]), so every per-provider rule (its shared folder, its TPM, its disks, its nested
//! virtualization switch, its console, how it boots, its bridges) is a field of that row, and a
//! new provider is a new variant whose row the compiler asks for. A provider that can't run the
//! VMs as the spec asks says so here (`provider_refusal`) instead of ignoring it.

use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::model::Arch;

use super::{boot, bridged, hyperv, qemu, utm, windows};

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
#[expect(
    clippy::struct_excessive_bools,
    reason = "a provider's row of the table: each switch is a yes or no Isoloom knows of it, read on its own"
)]
pub(crate) struct Profile {
    /// The name Vagrant knows it by (`vagrant up --provider <id>`, `config.vm.provider`).
    pub(crate) id: &'static str,
    /// Its name in a refusal's prose (`Hyper-V`).
    pub(crate) label: &'static str,
    /// Its two-way shared folder (`host_folders`).
    pub(crate) share: Share,
    /// Whether it gives a VM a virtual TPM 2.0.
    pub(crate) tpm: bool,
    /// Whether its plugin attaches a VM's data disks.
    pub(crate) attaches_disks: bool,
    /// Whether Isoloom bridges a network under it; `false` refuses a bridged network.
    pub(crate) bridges: bool,
    /// Whether each copy of the environment has networks of its own under it (VirtualBox's
    /// internal networks, Hyper-V's private switches, QEMU's hubs, UTM's vmnet networks,
    /// ESXi's port groups); `false` where a network is this computer's, one per address block
    /// (libvirt's bridges, VMware's vmnets, Parallels' host-only networks), so an instance
    /// can't have one beside the environment as written.
    pub(crate) own_networks: bool,
    /// What else it refuses of the environment's networks and boxes (QEMU's links, Hyper-V's
    /// and UTM's boxes).
    pub(crate) networks: Option<fn(&Environment) -> Option<String>>,
    /// How it boots a machine that boots its own way.
    pub(crate) boot: OwnBoot,
    /// The architecture of the host its VMs run on, when that isn't the computer Vagrant runs on
    /// (ESXi), whose architecture Isoloom doesn't know when it writes the Vagrantfile.
    pub(crate) remote_arch: Option<Arch>,
    /// Its nested virtualization switch.
    pub(crate) nested: Nested,
}

/// The architecture of an ESXi host, which runs the VMs of `vmware_esxi` (not the computer
/// Vagrant runs on): ESXi is x86-64 (ESXi-Arm is a rare, separate build Isoloom doesn't target).
pub(crate) const ESXI_ARCH: Arch = Arch::Amd64;

/// QEMU's links (one network per VM) and bridges.
fn qemu_networks(env: &Environment) -> Option<String> {
    qemu::refusal(super::qemu_links(env))
}

/// The defaults of a row: a desktop hypervisor on this computer that shares, nests, attaches
/// disks, and bridges.
const DESKTOP: Profile = Profile {
    id: "",
    label: "",
    share: Share::None(""),
    tpm: false,
    attaches_disks: true,
    bridges: true,
    own_networks: true,
    networks: None,
    boot: OwnBoot::BoxAsIs,
    remote_arch: None,
    nested: Nested::None,
};

impl Provider {
    /// Every provider the Vagrantfile has a block for, in the order the Vagrantfile writes
    /// their shared folders.
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
                id: "virtualbox",
                label: "VirtualBox",
                share: Share::Both {
                    kind: "virtualbox",
                    ro_option: "mount_options",
                },
                tpm: true,
                nested: Nested::Customize("modifyvm", "--nested-hw-virt"),
                ..DESKTOP
            },
            Provider::VmwareDesktop => Profile {
                id: "vmware_desktop",
                label: "VMware Desktop",
                share: Share::Both {
                    kind: "vmware",
                    ro_option: "mount_options",
                },
                nested: Nested::Vmx,
                own_networks: false,
                ..DESKTOP
            },
            Provider::HyperV => Profile {
                id: "hyperv",
                label: "Hyper-V",
                share: Share::None(
                    "the `hyperv` provider's only two-way shared folder is SMB, which asks for a Windows account's password at every `vagrant up`",
                ),
                tpm: true,
                bridges: false,
                networks: Some(hyperv::refusal),
                nested: Nested::Flag("enable_virtualization_extensions"),
                ..DESKTOP
            },
            Provider::Parallels => Profile {
                id: "parallels",
                label: "Parallels",
                share: Share::Both {
                    kind: "parallels",
                    ro_option: "mount_options",
                },
                attaches_disks: false,
                own_networks: false,
                nested: Nested::Customize("set", "--nested-virt"),
                ..DESKTOP
            },
            Provider::Libvirt => Profile {
                id: "libvirt",
                label: "libvirt",
                share: Share::Both {
                    kind: "virtiofs",
                    ro_option: "mount_opts",
                },
                tpm: true,
                bridges: false,
                own_networks: false,
                boot: OwnBoot::Own,
                nested: Nested::Flag("nested"),
                ..DESKTOP
            },
            Provider::Qemu => Profile {
                id: "qemu",
                label: "QEMU",
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
                id: "utm",
                label: "UTM",
                share: Share::Both {
                    kind: "utm",
                    ro_option: "mount_options",
                },
                // vagrant_utm attaches no data disk; its VMs' networks are Isoloom's own (each an
                // isolated vmnet network, `utm`), on UTM boxes only, and it bridges none.
                attaches_disks: false,
                bridges: false,
                networks: Some(utm::refusal),
                ..DESKTOP
            },
            Provider::VmwareEsxi => Profile {
                id: "vmware_esxi",
                label: "ESXi",
                share: Share::None("the `vmware_esxi` provider's VMs run on the ESXi host, not on this computer"),
                remote_arch: Some(ESXI_ARCH),
                nested: Nested::CustomVmx,
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
}

/// Why the VMs of `env`, planned for the `vagrant` target, can't all run under Vagrant's
/// `provider` (as `vagrant up --provider` names it), if they can't: a machine requiring a TPM the
/// provider doesn't give, then QEMU's networks, Hyper-V's and UTM's boxes and the bridged
/// networks of libvirt, QEMU, Hyper-V and UTM, then a machine's own boot, which
/// only libvirt (and QEMU, for UEFI's format) boots, then a machine's host folders, which Hyper-V
/// and ESXi don't share (nor QEMU read-only), then a machine of another architecture than ESXi's
/// on ESXi, then a machine's data disks, which QEMU's, Parallels' and UTM's plugins don't attach.
/// A spec that can't be planned at all (a machine's configuration that doesn't resolve) is
/// refused with why.
#[must_use]
pub fn provider_refusal(env: &Environment, provider: &str) -> Option<String> {
    // A provider Isoloom doesn't know has no block to refuse anything in.
    let provider = Provider::parse(provider)?;
    windows::tpm_refusal(env, provider)
        .or_else(|| networks_refusal(env, provider))
        .or_else(|| instance_refusal(env, provider))
        .or_else(|| boot::refusal(env, provider))
        .or_else(|| crate::host_folders::refusal(env, provider))
        .or_else(|| arch_refusal(env, provider))
        .or_else(|| disk_refusal(env, provider))
}

/// Why an instance of the environment can't have its networks under `provider`, if it can't:
/// one that makes a network of this computer per address block, which the environment as written
/// (and every other instance) makes on the same block.
fn instance_refusal(env: &Environment, provider: Provider) -> Option<String> {
    let n = env.instance?;
    let profile = provider.profile();
    if profile.own_networks {
        return None;
    }
    let spec = env.spec();
    let (name, net) = env
        .networks
        .iter()
        .filter(|(name, _)| spec.networks.get(name.as_str()).is_some_and(|net| !net.is_outside()))
        .find(|(name, _)| env.vms().any(|vm| vm.plan.addresses.contains_key(name.as_str())))?;
    Some(format!(
        "network `{name}` ({}) is a network of this computer under {}, one per address block, which the environment as written makes too: instance {n} can't have its own (VirtualBox, Hyper-V, QEMU and UTM give each copy networks of its own)",
        net.cidr, profile.label
    ))
}

/// The providers whose networks are this computer's ([`Profile::own_networks`]), as Vagrant
/// names them.
pub(crate) fn sharing_networks() -> Vec<&'static str> {
    Provider::profiles().filter(|p| !p.own_networks).map(|p| p.id).collect()
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

/// Why `provider` can't give this environment's VMs their data disks, if it can't: QEMU's,
/// Parallels' and UTM's plugins attach none.
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
/// network only, Hyper-V needs a Hyper-V build of each box and UTM a UTM box, and Isoloom bridges
/// no network on QEMU, Hyper-V, libvirt or UTM.
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
            assert_eq!(Provider::parse(p.id()), Some(p));
        }
        assert_eq!(Provider::parse("docker"), None);
        let ids: std::collections::BTreeSet<&str> = Provider::ALL.iter().map(|p| p.id()).collect();
        assert_eq!(ids.len(), Provider::ALL.len());
    }

    /// An environment with a VM on `lan`, planned for Vagrant as itself or as `instance`.
    fn planned(instance: Option<u8>) -> isoloom_plan::Environment {
        let lab = isoloom_plan::Lab::parse(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.20.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12, app: { steps: [{ run: s.sh }] } }\n",
        )
        .expect("the spec parses");
        lab.plan(
            &lab.for_target(isoloom_spec::model::Target::Vagrant),
            isoloom_spec::model::Target::Vagrant,
            instance,
        )
        .expect("the spec plans")
    }

    /// Two instances on one computer each make their networks under the provider: where a
    /// network is the computer's, one per block (libvirt, VMware, Parallels), the second would
    /// fail to start it, or share the first's.
    #[test]
    fn an_instance_is_refused_where_a_network_is_the_computers_and_taken_elsewhere() {
        let as_written = planned(None);
        let instance = planned(Some(2));
        for id in ["libvirt", "vmware_desktop", "parallels"] {
            assert_eq!(super::provider_refusal(&as_written, id), None, "{id}");
            let why = super::provider_refusal(&instance, id).unwrap_or_default();
            assert!(
                why.starts_with("network `lan` (10.20.0.0/24) is a network of this computer under ") && why.contains("instance 2"),
                "{id}: {why}"
            );
        }
        for id in ["virtualbox", "hyperv", "qemu", "utm", "vmware_esxi"] {
            assert_eq!(super::provider_refusal(&instance, id), None, "{id}");
        }
        assert_eq!(super::sharing_networks(), ["vmware_desktop", "parallels", "libvirt"]);
    }

    #[test]
    fn only_virtualbox_libvirt_and_hyperv_give_a_tpm() {
        let with: Vec<&str> = Provider::ALL.into_iter().filter(|p| p.has_tpm()).map(Provider::id).collect();
        assert_eq!(with, ["virtualbox", "hyperv", "libvirt"]);
    }
}
