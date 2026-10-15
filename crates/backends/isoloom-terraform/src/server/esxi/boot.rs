//! A machine's own boot on ESXi (`vm.boot`): its firmware, a BIOS or EFI (`boot_firmware`).
//! ESXi boots the VM's disk through that firmware and nothing else: a kernel, a firmware file,
//! a boot order or a network boot of the machine's own are refused, with why.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_spec::model::Firmware;

use crate::machine::vms;
use crate::server::boot::{Field, HOST_FILES, first_set};

/// Why ESXi can't boot a machine its own way, if it can't: the first field it has no setting
/// for.
pub(super) fn refusal(env: &Environment) -> Option<String> {
    vms(env).find_map(|(name, mp)| {
        let b = mp.boot.as_ref()?;
        let field = first_set(b, HOST_FILES.into_iter().chain([Field::FirmwareFormat, Field::Order, Field::Network]))?;
        Some(format!(
            "machine `{name}` sets `hardware.boot.{field}`: ESXi boots the VM's disk through its own firmware (`firmware` applies, as `boot_firmware`); Vagrant's libvirt provider boots it"
        ))
    })
}

/// The VM's `boot_firmware`, when the machine sets its firmware.
pub(super) fn firmware(mp: &MachinePlan) -> Option<&'static str> {
    Some(match mp.boot.as_ref()?.firmware? {
        Firmware::Bios => "bios",
        Firmware::Uefi => "efi",
    })
}
