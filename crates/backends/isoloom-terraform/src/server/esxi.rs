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
//!   internet (NAT).
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
//!   output gives their addresses, and `isoloom test esxi` pipes the check runners in
//!   `.isoloom/esxi/checks/<position>.sh` to them.

mod base;
mod boot;
mod disks;
mod guest;
mod machines;
mod outside;
mod router;
mod windows;

use isoloom_plan::{Capabilities, Environment};
use isoloom_render::GeneratedFile;
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_spec::images::is_windows;
use isoloom_spec::model::Target;

use crate::limits::Limits;
use crate::machine::{controller, vms};
use crate::module::{ProjectCopy, Usage};
use crate::{Result, target};

/// The OS of the router and the controller: Ubuntu's OVA has cloud-init and open-vm-tools,
/// which reports the router's uplink address.
const SERVICE_OS: &str = "ubuntu-24.04";

/// A VM's network interfaces, at most: the router needs one per network and the uplink.
const MAX_NICS: usize = 10;

/// What ESXi can't host yet.
const LIMITS: Limits = Limits {
    arm64: Some("ESXi runs x86-64 VMs only"),
    gateways: Some("networks with a `gateway` machine on ESXi come later"),
    playbook_checks: None,
    tc: None,
    ipv6: None,
};

/// Ubuntu's own cloud OVAs: the default `images`.
fn default_image(os: &str) -> Option<&'static str> {
    Some(match os {
        "ubuntu-20.04" => "https://cloud-images.ubuntu.com/focal/current/focal-server-cloudimg-amd64.ova",
        "ubuntu-22.04" => "https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.ova",
        "ubuntu-24.04" => "https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.ova",
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
/// no OVA here, then what a Windows machine can't have yet, a boot of its own ESXi has no
/// setting for, more data disks than slots, and more networks than a VM has interfaces for.
fn refusal(env: &Environment) -> Option<String> {
    LIMITS
        .refusal(env)
        .or_else(|| {
            vms(env).find_map(|(name, mp)| {
                let os = mp.os.as_deref().unwrap_or_default();
                (!is_windows(os) && !LINUX.contains(&os)).then(|| {
                    format!("machine `{name}`: no ESXi image for `{os}` yet (Debian, Ubuntu, Rocky, AlmaLinux: an OVA with cloud-init and open-vm-tools; Windows: an OVA with cloudbase-init)")
                })
            })
        })
        .or_else(|| windows::refusal(env))
        .or_else(|| boot::refusal(env))
        .or_else(|| disks::refusal(env))
        .or_else(|| interfaces(env))
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
    vms(env)
        .find(|(_, mp)| mp.addresses.len() + mp.outside.len() > MAX_NICS)
        .map(|(name, _)| format!("machine `{name}`: an ESXi VM has at most {MAX_NICS} network interfaces"))
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
    /// limits, which the provider has no setting for) is refused.
    fn capabilities(&self) -> Capabilities {
        Capabilities::of(super::features())
    }

    fn render(&self, env: &Environment) -> std::result::Result<Files, RenderError> {
        super::render(Target::Esxi, &self.capabilities(), env, refusal, module)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        target::refusal(env, &self.capabilities(), || refusal(env))
    }
}

/// The module: its base (settings, networks), the router, the machines, the controller, the
/// outputs.
fn module(env: &Environment) -> Result<GeneratedFile> {
    let usage = Usage {
        before: "# Needs:  ovftool on this machine, SSH enabled on the ESXi host\n",
        apply: " -var esxi_hostname=<host> -var esxi_password=<password>",
        ..Usage::with_key(Target::Esxi.id(), "")
    };
    let mut m = crate::module::start(&usage);
    base::blocks(env, &mut m);
    router::blocks(env, &mut m);
    for (i, (name, mp)) in (0..).zip(vms(env)) {
        machines::machine(env, (name, mp), i, &mut m);
    }
    if controller::needed(env) {
        machines::controller(env, &mut m);
    }
    router::outputs(env, &mut m);
    crate::module::file(Target::Esxi, Target::Esxi.id(), m, ProjectCopy::Archive)
}

#[cfg(test)]
mod tests;
