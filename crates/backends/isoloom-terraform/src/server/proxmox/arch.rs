//! arm64 machines (`arch: arm64`) on an x86-64 server: aarch64 VMs that QEMU emulates (no KVM
//! for another architecture, so they run several times slower), from Debian's and Ubuntu's
//! arm64 cloud images. Proxmox lets only `root@pam` set a VM's architecture (log in with
//! `proxmox_username` and `proxmox_password`, not a token), and boots it through the server's
//! aarch64 UEFI firmware (the `pve-edk2-firmware-aarch64` package).

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Block, BlockBuilder, number, reference, string};
use isoloom_spec::model::Arch;

use crate::machine::vms;
use crate::module::ident;

/// The arm64 cloud image of an OS, where one is published.
pub(super) fn image_url(os: &str) -> Option<&'static str> {
    Some(match os {
        "debian-11" => "https://cloud.debian.org/images/cloud/bullseye/latest/debian-11-genericcloud-arm64.qcow2",
        "debian-12" => "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-arm64.qcow2",
        "debian-13" => "https://cloud.debian.org/images/cloud/trixie/latest/debian-13-genericcloud-arm64.qcow2",
        "ubuntu-20.04" => "https://cloud-images.ubuntu.com/focal/current/focal-server-cloudimg-arm64.img",
        "ubuntu-22.04" => "https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-arm64.img",
        "ubuntu-24.04" => "https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-arm64.img",
        _ => return None,
    })
}

/// Whether the machine is an arm64 VM.
pub(super) fn is_arm(mp: &MachinePlan) -> bool {
    mp.arch == Arch::Arm64
}

/// The resource name of an OS's cloud image for the machine's architecture.
pub(super) fn image_res(mp: &MachinePlan) -> String {
    let o = ident(mp.os.as_deref().unwrap_or_default());
    if is_arm(mp) { format!("{o}_arm64") } else { o }
}

/// Why an arm64 machine can't run here, if one can't: Windows, or an OS with no arm64 image of
/// Isoloom's and none of its own.
pub(super) fn refusal(env: &Environment) -> Option<String> {
    vms(env).filter(|(_, mp)| is_arm(mp)).find_map(|(name, mp)| {
        let o = mp.os.as_deref().unwrap_or_default();
        (mp.is_windows() || (image_url(o).is_none() && !super::image::has_own(mp))).then(|| {
            format!("machine `{name}`: no arm64 Proxmox image for `{o}` (Debian 11 to 13, Ubuntu 20.04 to 24.04, or `overrides.proxmox.machines.{name}.image`)")
        })
    })
}

/// A VM's CPU: the server's own for an amd64 VM (nested virtualization included), QEMU's
/// emulated `max` aarch64 CPU for an arm64 one.
pub(super) fn cpu(cores: u32, arm: bool) -> Block {
    let b = Block::builder("cpu").add_attribute(("cores", number(cores.into())));
    if arm {
        b.add_attribute(("architecture", string("aarch64"))).add_attribute(("type", string("max")))
    } else {
        b.add_attribute(("type", string("host")))
    }
    .build()
}

/// What an aarch64 VM needs besides: UEFI (the `virt` machine has no BIOS), its cloud-init
/// drive on SCSI (it has no IDE), and its console on the serial port (no VGA).
pub(super) fn arm(b: BlockBuilder) -> BlockBuilder {
    b.add_attribute(("bios", string("ovmf")))
        .add_block(Block::builder("vga").add_attribute(("type", string("serial0"))).build())
}

/// Its EFI variables' disk.
pub(super) fn efi_disk() -> Block {
    Block::builder("efi_disk")
        .add_attribute(("datastore_id", reference("var.datastore")))
        .add_attribute(("type", string("4m")))
        .build()
}
