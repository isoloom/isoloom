//! A VM's own boot (`vm.boot`): a kernel booted directly, a UEFI firmware file, the boot devices'
//! order and a network boot. Most machines boot their image the way it was built; these are for
//! the labs that study the boot itself (a custom kernel, a board's device tree, PXE).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How a VM boots, instead of the way its image does. Paths are on the hypervisor's host (the
/// hypervisor reads them, not Isoloom), so they are absolute. Vagrant's libvirt provider boots
/// every field and its QEMU provider `firmware_format`; the other targets refuse a machine with
/// one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Boot {
    /// A kernel to boot directly, instead of the image's boot loader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel: Option<String>,
    /// The initial RAM disk booted with `kernel`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initrd: Option<String>,
    /// The command line passed to `kernel` (`console=ttyS0 root=/dev/vda1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmdline: Option<String>,
    /// A device tree blob for `kernel` (ARM boards).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dtb: Option<String>,
    /// The firmware the VM boots through: `bios`, or `uefi` (on libvirt, with its file in
    /// `loader`; Proxmox and QEMU bring their own).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware: Option<Firmware>,
    /// A UEFI firmware file the VM boots through (OVMF's `OVMF_CODE.fd`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader: Option<String>,
    /// The file that keeps the firmware's variables between boots (with `loader`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nvram: Option<String>,
    /// The format of the UEFI firmware files the hypervisor loads: `raw` (QEMU's own, as
    /// Homebrew ships them) or `qcow2` (some distributions'). Vagrant's QEMU provider needs it
    /// to load them; libvirt reads the format from the file itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware_format: Option<FirmwareFormat>,
    /// The devices to boot from, in order (`[network, hd]` tries a network boot first).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<BootDevice>,
    /// A network boot served on the machine's first network: its TFTP folder and the file
    /// DHCP hands out. `order` must list `network`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkBoot>,
}

impl Boot {
    /// The fields set besides `firmware_format` and `firmware: uefi`, by name: what Vagrant's
    /// QEMU provider can't boot (it boots the box's disk through its own UEFI firmware).
    pub fn beyond_firmware_format(&self) -> Vec<&'static str> {
        let set = [
            ("firmware", self.firmware == Some(Firmware::Bios)),
            ("kernel", self.kernel.is_some()),
            ("initrd", self.initrd.is_some()),
            ("cmdline", self.cmdline.is_some()),
            ("dtb", self.dtb.is_some()),
            ("loader", self.loader.is_some()),
            ("nvram", self.nvram.is_some()),
            ("order", !self.order.is_empty()),
            ("network", self.network.is_some()),
        ];
        set.into_iter().filter(|(_, on)| *on).map(|(name, _)| name).collect()
    }
}

/// The firmware a VM boots through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Firmware {
    /// A legacy BIOS (SeaBIOS).
    Bios,
    /// UEFI (OVMF).
    Uefi,
}

/// The format of a UEFI firmware file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FirmwareFormat {
    /// A raw image (QEMU's `edk2-*-code.fd`).
    Raw,
    /// A qcow2 image.
    Qcow2,
}

impl FirmwareFormat {
    /// The name the hypervisors use.
    pub fn id(self) -> &'static str {
        match self {
            FirmwareFormat::Raw => "raw",
            FirmwareFormat::Qcow2 => "qcow2",
        }
    }
}

/// A device a VM boots from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BootDevice {
    /// The machine's disk.
    Hd,
    /// The network (PXE).
    Network,
    /// A CD-ROM drive.
    Cdrom,
}

impl BootDevice {
    /// The name libvirt gives it.
    pub fn id(self) -> &'static str {
        match self {
            BootDevice::Hd => "hd",
            BootDevice::Network => "network",
            BootDevice::Cdrom => "cdrom",
        }
    }
}

/// A network boot: what the network's DHCP and TFTP server hand a booting machine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkBoot {
    /// The folder the network's TFTP server serves, on the hypervisor's host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tftp_root: Option<String>,
    /// The boot file DHCP hands out (`pxelinux.0`), from the TFTP folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The address of the TFTP server holding `file`, when it isn't the host's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
}
