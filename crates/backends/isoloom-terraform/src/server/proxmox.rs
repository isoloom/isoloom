//! The `proxmox` target: Terraform for the bpg/proxmox provider, in `.isoloom/proxmox/main.tf`,
//! rendered from the plan through the VM core.
//!
//! - The environment gets its own SDN simple zone, and a VNet per network. No SDN subnets: a
//!   subnet's gateway lives on the host bridge, which would route between networks.
//! - A router VM sits on the server's uplink bridge and on every network at its router address.
//!   It forwards between networks with the `reach` rules (nftables), and gives the machines the
//!   internet (NAT).
//! - Each machine is a VM from a cloud image, with its fixed address on each network and the
//!   router as its gateway. Its set-up is a cloud-init document (the core's cloud-init
//!   transport): the project written to /opt/isoloom, then its steps from the plan. After a
//!   `reboot` step, cloud-init restarts the machine and a unit runs the rest at the next boot.
//! - A Windows machine is a full clone of a template you give per OS, set up by cloudbase-init
//!   and reached by the controller over WinRM as `isoloom` (its `windows` module).
//! - A shared volume may be a folder of the server instead (`virtiofs`), mounted by each VM
//!   that uses it (its `virtiofs` module).
//! - A machine with a virtual TPM (`vm.tpm`, or Windows 11's own) has its state on the VM
//!   datastore (`tpm_state`).
//! - The connection, the node, the datastores and a `slot` (unique per environment on the
//!   server: SDN ids are short) are variables.
//! - Machines are reached through the router (`ssh -J isoloom@<address>`, the `isoloom` user
//!   with `ssh_public_key`): the `machines` output gives their addresses, and the check runners
//!   in `.isoloom/proxmox/checks/<position>.sh` are piped to them by `isoloom test proxmox`
//!   (the `checks` output says which runs where).

mod arch;
mod base;
mod boot;
mod controller;
pub(crate) mod docker_host;
mod firewall;
mod gpus;
mod image;
mod links;
mod lxc;
mod machine;
mod manifest;
mod outside;
mod router;
mod uplink;
mod virtiofs;
mod windows;

use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::Error as RenderError;
use isoloom_render::backend::{Backend, Files};
use isoloom_render::hcl_builder::Module;
use isoloom_spec::model::Target;

use super::{Parts, PinnedImage};
use crate::limits::Limits;
use crate::machine::{cloudbase_init, controller as core_controller, vms};
use crate::module::{ProjectCopy, Usage};

/// What Proxmox can't host yet.
const LIMITS: Limits = Limits {
    // Emulated by QEMU (see `arch`), with an arm64 cloud image.
    arm64: None,
    gateways: Some("networks with a `gateway` machine on Proxmox come later"),
    playbook_checks: None,
    tc: None,
    ipv6: None,
};

/// Debian 12's amd64 cloud image, pinned. Shared with the Docker-host target ([`docker_host`]).
pub(super) const DEBIAN_12_AMD64: PinnedImage = PinnedImage::sha512(
    "https://cloud.debian.org/images/cloud/bookworm/20261015-2632/debian-12-genericcloud-amd64-20261015-2632.qcow2",
    "56c0bd55b6623f2739f377e983d2872e7b208d0cb9c3ba4e93d3f72929026da03637c598b3bd472b29998c31c8a4c17d8e6ae1c556852a8b83a66a09bf5c2b37",
);

/// Pinned amd64 cloud images by OS name (Linux for now), each at a dated release with the
/// checksum its publisher lists (resolved 2026-10-17; see [`PinnedImage`]).
fn pinned(os: &str) -> Option<PinnedImage> {
    Some(match os {
        "debian-11" => PinnedImage::sha512(
            "https://cloud.debian.org/images/cloud/bullseye/20260821-2577/debian-11-genericcloud-amd64-20260821-2577.qcow2",
            "eca21286cadc03444eba75c35c30869707c39867b7cd4a78fa315a021243e21898d2675f5de2c5d8bd1e33753d01a7364cd1f479dbcbd7ea83d72612219ffda5",
        ),
        "debian-12" => DEBIAN_12_AMD64,
        "debian-13" => PinnedImage::sha512(
            "https://cloud.debian.org/images/cloud/trixie/20261015-2632/debian-13-genericcloud-amd64-20261015-2632.qcow2",
            "7f1d06b629fa01241ec5817735777d0554554ff8c8d9dc27f9eac65ba3c3a6f859deb6079ebc9c65795d1955abd00a77aa3ff9cb84ae6307dfbbd8b2e398dab2",
        ),
        // End of life, still published. 14.04's cloud-init (0.7.5) is too old for the snippet.
        "ubuntu-16.04" => PinnedImage::sha256(
            "https://cloud-images.ubuntu.com/xenial/20211001/xenial-server-cloudimg-amd64-disk1.img",
            "fff2494a70bcaffb59f26f71ec49e137dd87d319341c35832dbe0ea65ff15140",
        ),
        "ubuntu-18.04" => PinnedImage::sha256(
            "https://cloud-images.ubuntu.com/bionic/20230607/bionic-server-cloudimg-amd64.img",
            "8dd2e6b5e5aad20c3f836123b300cba9861249408cbb07c359145a65d6bab6b6",
        ),
        "ubuntu-20.04" => PinnedImage::sha256(
            "https://cloud-images.ubuntu.com/focal/20250624/focal-server-cloudimg-amd64.img",
            "18f2977d77dfea1b74aee14533bd21c34f789139e949c57023b7364894b7e5e9",
        ),
        "ubuntu-22.04" => PinnedImage::sha256(
            "https://cloud-images.ubuntu.com/jammy/20261004/jammy-server-cloudimg-amd64.img",
            "012d81fade7e8ff4428fc35dfeee711d66d1f47d5ea9c5c8ced96a85b959835c",
        ),
        "ubuntu-24.04" => PinnedImage::sha256(
            "https://cloud-images.ubuntu.com/noble/20260926/noble-server-cloudimg-amd64.img",
            "6a81c37564db9b1ee84e141922625e1d7c5b389b99bb3c572e0243607d5bb4d2",
        ),
        "rocky-9" => PinnedImage::sha256(
            "https://dl.rockylinux.org/pub/rocky/9/images/x86_64/Rocky-9-GenericCloud-Base-9.8-20260525.0.x86_64.qcow2",
            "92c206cc6f790c61583247eefe87890f8828420662c17cacf247cec78ab4eec8",
        ),
        "almalinux-9" => PinnedImage::sha256(
            "https://repo.almalinux.org/almalinux/9/cloud/x86_64/images/AlmaLinux-9-GenericCloud-9.8-20261005.x86_64.qcow2",
            "25942b554efaac077e2f60d6bb27bd3332c171454cc21bbd33e5f79244d7570f",
        ),
        "centos-7" => PinnedImage::sha256(
            "https://cloud.centos.org/centos/7/images/CentOS-7-x86_64-GenericCloud.qcow2",
            "284aab2b23d91318f169ff464bce4d53404a15a0618ceb34562838c59af4adea",
        ),
        _ => return None,
    })
}

/// Why Proxmox can't run this environment, if it can't: its limits, then a Linux machine with
/// no cloud image (its own image stands in for its OS's), then what a Windows machine can't
/// have here yet, then a boot of its own Proxmox has no setting for.
fn refusal(env: &Environment) -> Option<String> {
    LIMITS.refusal(env).or_else(|| gpus::refusal(env)).or_else(|| {
        super::without_image(env, |os, mp| pinned(os).is_some() || image::has_own(mp))
            .map(|(name, os)| {
                format!("machine `{name}`: no Proxmox image for `{os}` yet (Debian, Ubuntu, Rocky, AlmaLinux, CentOS 7, Windows from a template)")
            })
            .or_else(|| cloudbase_init::refusal(env, "Proxmox"))
            .or_else(|| image::refusal(env))
            .or_else(|| boot::refusal(env))
            .or_else(|| arch::refusal(env))
            .or_else(|| virtiofs::refusal(env))
            .or_else(|| lxc::refusal(env))
    })
}

/// The `proxmox` backend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ProxmoxBackend;

impl Backend for ProxmoxBackend {
    fn target(&self) -> Target {
        Target::Proxmox
    }

    /// What a server target gives (`crate::server::features`), and a virtual TPM, GPUs passed
    /// through, nesting, rate limits and VMs waiting for LXC containers.
    fn capabilities(&self) -> Capabilities {
        Capabilities::of(super::features().chain([
            Feature::Tpm,
            // Passed through by PCI id or resource mapping (`gpus.ids`, refused without).
            Feature::VmGpus,
            // The VMs take the server's CPU (`cpu.type = host`).
            Feature::NestedVirtualization,
            // On every device, as its network's MTU is.
            Feature::RateLimit,
            // A VM may wait for an LXC container (`docker.lxc`) as for a VM.
            Feature::VmAfterContainer,
        ]))
    }

    fn render(&self, env: &Environment) -> std::result::Result<Files, RenderError> {
        let checked = lxc::checked(env);
        let mounted = virtiofs::mounted(&checked);
        PARTS.render(&self.capabilities(), &mounted)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        PARTS.refusal(&self.capabilities(), env)
    }

    /// The run manifest (`.isoloom/proxmox/run.json`): Terraform applies the infrastructure, the
    /// runner reads each machine's ready file and runs the checks over SSH through the router.
    fn run_manifest(&self, env: &Environment) -> std::result::Result<Option<isoloom_manifest::Manifest>, RenderError> {
        let checked = lxc::checked(env);
        let mounted = virtiofs::mounted(&checked);
        Ok(Some(manifest::manifest(&mounted)?))
    }
}

/// The module: its base (settings, networks, images), the router, the machines, the LXC
/// containers, the controller, the outputs.
const PARTS: Parts = Parts {
    target: Target::Proxmox,
    refusal,
    usage: |folder| Usage {
        apply: " -var proxmox_endpoint=https://<server>:8006/ -var proxmox_api_token=<user@realm!name=secret>",
        key: None,
        ..Usage::with_key(folder, "")
    },
    copy: ProjectCopy::Files,
    base: base::blocks,
    router: router::blocks,
    guests,
    outputs: router::outputs,
};

/// The VMs, the LXC containers, then the controller.
fn guests(env: &Environment, m: &mut Module) {
    for (name, mp) in vms(env) {
        machine::blocks(env, name, mp, m);
    }
    lxc::blocks(env, m);
    if core_controller::needed(env) {
        controller::blocks(env, m);
    }
}
