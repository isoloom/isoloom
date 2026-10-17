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
use crate::server::PinnedImage;

/// The arm64 cloud image of an OS, where one is published: pinned to a dated release with its
/// publisher's checksum (resolved 2026-10-17; see [`PinnedImage`]).
pub(super) fn pinned(os: &str) -> Option<PinnedImage> {
    Some(match os {
        "debian-11" => PinnedImage::sha512(
            "https://cloud.debian.org/images/cloud/bullseye/20260821-2577/debian-11-genericcloud-arm64-20260821-2577.qcow2",
            "7163c13dbaed4d941a815ba09f28f347f47c519c79d03c634a72a5fc5d7656a18093461e914e3bc4d060f9a679b44f57bbe5377695b474383687a9aebc3d1831",
        ),
        "debian-12" => PinnedImage::sha512(
            "https://cloud.debian.org/images/cloud/bookworm/20261015-2632/debian-12-genericcloud-arm64-20261015-2632.qcow2",
            "e69fe6ec73d73af4de0523b661078774d87c270e72ba99f5e262a54d2de8fff065b5dde145b1671e4c304be333c25e21e20a997742a4b389b7a36c09272131f4",
        ),
        "debian-13" => PinnedImage::sha512(
            "https://cloud.debian.org/images/cloud/trixie/20261015-2632/debian-13-genericcloud-arm64-20261015-2632.qcow2",
            "cebe84e4dda9c86ace62eda0763a479e7367f78a64caa219190e572269589387c9245240c26f2cda954e7ada69618fe027783b74a880f2b163acf47b116e7f49",
        ),
        "ubuntu-20.04" => PinnedImage::sha256(
            "https://cloud-images.ubuntu.com/focal/20250624/focal-server-cloudimg-arm64.img",
            "d4603cd783e2578f838c63f7c3a8dae6b554bd6358870f2c8eb182aa51ade465",
        ),
        "ubuntu-22.04" => PinnedImage::sha256(
            "https://cloud-images.ubuntu.com/jammy/20261004/jammy-server-cloudimg-arm64.img",
            "86e13258765945ad792dac75fac7f02f4db9fa73a4a535ffa472266e7041fa19",
        ),
        "ubuntu-24.04" => PinnedImage::sha256(
            "https://cloud-images.ubuntu.com/noble/20260926/noble-server-cloudimg-arm64.img",
            "1d6bffe64b848468ac97f821d369a4846d983de1800ccf6b5ec8853e85cefc55",
        ),
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
        (mp.is_windows() || (pinned(o).is_none() && !super::image::has_own(mp))).then(|| {
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
