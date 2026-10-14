//! A machine's own boot on ESXi (`vm.boot`): its firmware, a BIOS or EFI (`boot_firmware`).
//! ESXi boots the VM's disk through that firmware and nothing else: a kernel, a firmware file,
//! a boot order or a network boot of the machine's own are refused, with why.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_spec::model::{BootDevice, Firmware};

use crate::vm::vms;

/// Why ESXi can't boot a machine its own way, if it can't: the first field it has no setting
/// for.
pub fn refusal(env: &Environment) -> Option<String> {
    vms(env).find_map(|(name, mp)| {
        let b = mp.boot.as_ref()?;
        let fields = [
            ("kernel", b.kernel.is_some()),
            ("initrd", b.initrd.is_some()),
            ("cmdline", b.cmdline.is_some()),
            ("dtb", b.dtb.is_some()),
            ("loader", b.loader.is_some()),
            ("nvram", b.nvram.is_some()),
            ("firmware_format", b.firmware_format.is_some()),
            ("order", b.order.iter().any(|d| *d != BootDevice::Hd)),
            ("network", b.network.is_some()),
        ];
        let (field, _) = fields.iter().find(|(_, set)| *set)?;
        Some(format!(
            "machine `{name}` sets `vm.boot.{field}`: ESXi boots the VM's disk through its own firmware (`firmware` applies, as `boot_firmware`); Vagrant's libvirt provider boots it"
        ))
    })
}

/// The VM's `boot_firmware`, when the machine sets its firmware.
pub fn firmware(mp: &MachinePlan) -> Option<&'static str> {
    Some(match mp.boot.as_ref()?.firmware? {
        Firmware::Bios => "bios",
        Firmware::Uefi => "efi",
    })
}
