//! The `esxi` target: Terraform for the josenk/esxi provider, in `.isoloom/esxi/main.tf`, for a
//! standalone ESXi host (no vCenter), rendered from the plan through the VM core.
//!
//! - Each network gets its own vSwitch, with no uplink, and one port group on it: networks
//!   never share a layer 2 segment, and nothing leaves the host but through the router. A
//!   network's `mtu` is its vSwitch's, and each guest interface's on it. Outside networks are
//!   an existing port group (`external`) or a port group on the host's LAN (`bridged`, see
//!   `outside`).
//! - A router VM sits on the host's own port group (`uplink_port_group`, DHCP) and on every
//!   network at its router address, as on Proxmox: it forwards between networks with the
//!   `reach` rules (nftables), forwards the published ports, and gives the machines the
//!   internet (NAT). Once the machines are set up, the module's seal closes the internet
//!   to the `internet: false` networks at the router, as on Proxmox.
//! - Each machine is a VM deployed from an OVA (`images`, Ubuntu's cloud OVAs by default). Its
//!   addresses (matched by MAC) and its `isoloom` user come from cloud-init's VMware data
//!   source (`guestinfo`). Terraform then sets it up over SSH through the router: the core's
//!   Linux transport (`remote-exec`), the project uploaded, then its steps from the plan. Its
//!   firmware is its own when `vm.boot.firmware` says so (`boot_firmware`), and its data disks
//!   are an `esxi_virtual_disk` each, attached by SCSI slot and formatted by its `Disks` step.
//! - A Windows machine is a VM from the OVA given for its OS, with its guest OS type
//!   (`guestos`), set up by cloudbase-init at its first boot as on Proxmox (see `windows`).
//! - The controller, when the environment has playbooks, is the core's controller over SSH
//!   through the router, with the same inventory as Proxmox.
//! - The host, its datastore, the uplink port group and a `slot` (unique per environment on
//!   the host: it names the vSwitches and the VMs, and makes the MACs) are variables.
//! - Machines are reached through the router (`ssh -J isoloom@<address>`): the `machines`
//!   output gives their addresses, and the check runners in
//!   `.isoloom/esxi/checks/<position>.sh` are piped to them over SSH through the router (the
//!   `checks` output says which runs where).
//! - Terraform makes the environment, sets every machine up over SSH and, once they are, seals
//!   the `internet: false` networks at the router (`crate::server::router::seal`): the apply ends
//!   with the environment ready. The module's files are the whole output: no run manifest.
//! - A Linux machine's secrets and inputs as files come from the module's variables (sensitive
//!   ones for the secrets), written by cloud-init from its guestinfo user data; Terraform keeps them in its state, as
//!   it does every variable. A Windows machine's are refused (`super::secrets_refusal`).

mod base;
mod boot;
mod disks;
mod guest;
mod machines;
mod networks;
mod outside;
mod router;
mod windows;

use isoloom_plan::{Capabilities, Environment, Vm};
use isoloom_render::Error as RenderError;
use isoloom_render::backend::{Backend, Files};
use isoloom_spec::model::Target;

use super::Parts;
use crate::limits::Limits;
use crate::module::{ProjectCopy, Usage};
use crate::server::PinnedImage;

/// The OS of the router and the controller: Ubuntu's OVA has cloud-init and open-vm-tools,
/// which reports the router's uplink address.
const SERVICE_OS: &str = "ubuntu-24.04";

/// A VM's network interfaces, at most: the router needs one per network and the uplink.
const MAX_NICS: usize = 10;

/// What ESXi can't host.
const LIMITS: Limits = Limits {
    arm64: Some("ESXi runs x86-64 VMs only"),
    gateways: Some("Isoloom routes no network through a `gateway` machine on ESXi"),
    playbook_checks: None,
    tc: None,
    ipv6: None,
};

/// Ubuntu's own cloud OVAs: the default `images`, pinned to a dated release (never the floating
/// `current`) so a generated environment keeps booting the same OS build, each with the SHA-256
/// Ubuntu publishes for it. The `josenk/esxi` provider deploys an OVA from
/// `ovf_source` with no checksum slot, so the module checks none: the `images` variable's
/// description names each default's SHA-256 for the user to check the download against. The
/// other ESXi path (`vagrant/vmware_esxi`) pins its boxes through Vagrant instead.
fn default_image(os: &str) -> Option<PinnedImage> {
    Some(match os {
        "ubuntu-20.04" => PinnedImage::sha256(
            "https://cloud-images.ubuntu.com/focal/20250624/focal-server-cloudimg-amd64.ova",
            "1d4521b2df38914965064537e31f7fbb1c3fabeaca789e3350b79a0b1a0838fc",
        ),
        "ubuntu-22.04" => PinnedImage::sha256(
            "https://cloud-images.ubuntu.com/jammy/20261004/jammy-server-cloudimg-amd64.ova",
            "ea42c32ec5e7ca343f0a5da30f242364adc75f564cbc8b9e61ab3e9c68337df7",
        ),
        "ubuntu-24.04" => PinnedImage::sha256(
            "https://cloud-images.ubuntu.com/noble/20260926/noble-server-cloudimg-amd64.ova",
            "513a22ebe3982b9387b038f8a2a6dad1af980d4623dca03511312e55ba620b1a",
        ),
        _ => return None,
    })
}

/// The Linux OSes a machine can run: Ubuntu from its own OVA, the others from an OVA the user
/// gives in `images` (with cloud-init's VMware data source and open-vm-tools).
const LINUX: [&str; 8] = [
    "debian-11",
    "debian-12",
    "debian-13",
    "ubuntu-20.04",
    "ubuntu-22.04",
    "ubuntu-24.04",
    "rocky-9",
    "almalinux-9",
];

/// Why ESXi can't run this environment, if it can't: its limits, then a machine whose OS has
/// no OVA here, then what a Windows machine can't have here, a boot of its own ESXi has no
/// setting for, more data disks than slots, and more networks than a VM has interfaces for.
fn refusal(env: &Environment) -> Option<String> {
    LIMITS
        .refusal(env)
        .or_else(|| {
            super::without_image(env, |os, _| LINUX.contains(&os)).map(|(name, os)| {
                format!("machine `{name}`: no ESXi image for `{os}` (Debian, Ubuntu, Rocky, AlmaLinux: an OVA with cloud-init and open-vm-tools; Windows: an OVA with cloudbase-init)")
            })
        })
        .or_else(|| windows::refusal(env))
        .or_else(|| boot::refusal(env))
        .or_else(|| disks::refusal(env))
        .or_else(|| interfaces(env))
        .or_else(|| super::secrets_refusal(env))
}

/// Why a VM would have more interfaces than ESXi gives it, if one would: the router needs one
/// per network and its uplink, a machine one per network it joins, outside ones included.
fn interfaces(env: &Environment) -> Option<String> {
    if env.networks.len() >= MAX_NICS {
        return Some(format!(
            "{} networks: an ESXi VM has at most {MAX_NICS} network interfaces, and the router needs one per network and its uplink",
            env.networks.len()
        ));
    }
    env.vms()
        .find(|Vm { plan: mp, .. }| mp.addresses.len() + mp.outside.len() > MAX_NICS)
        .map(|Vm { name, .. }| format!("machine `{name}`: an ESXi VM has at most {MAX_NICS} network interfaces"))
}

/// The `esxi` backend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct EsxiBackend;

impl Backend for EsxiBackend {
    fn target(&self) -> Target {
        Target::Esxi
    }

    /// What a server target gives (`crate::server::features`): the router, the controller, the
    /// machines' own steps and the provider. The rest (own images, TPM, GPUs, nesting, rate
    /// limits, which the provider has no setting for) is refused. A Linux machine's secrets come
    /// from the module's variables, written by cloud-init from its guestinfo user data
    /// (`super::secrets_refusal` says
    /// where they can't).
    fn capabilities(&self) -> Capabilities {
        Capabilities::of(super::features())
    }

    fn render(&self, env: &Environment) -> std::result::Result<Files, RenderError> {
        PARTS.render(&self.capabilities(), env)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        PARTS.refusal(&self.capabilities(), env)
    }
}

/// The module: its base (settings, networks), the router, the machines, the controller, the
/// outputs.
const PARTS: Parts = Parts {
    target: Target::Esxi,
    refusal,
    usage: |folder| Usage {
        before: "# Needs:  ovftool on this machine, SSH enabled on the ESXi host\n# Auth:   export TF_VAR_esxi_password=<password> (from the environment: never on a command line)\n",
        apply: " -var esxi_hostname=<host>",
        ..Usage::with_key(folder, "")
    },
    copy: ProjectCopy::Archive,
    base: base::blocks,
    router: router::blocks,
    guests: machines::guests,
    outputs: router::outputs,
};

#[cfg(test)]
mod tests;
