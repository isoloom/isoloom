//! A machine's own boot on Proxmox (`vm.boot`): its firmware (SeaBIOS, or OVMF with an EFI disk
//! for its variables) and its boot order. Proxmox boots the VM's disk through its own firmware:
//! a kernel, a firmware file or a network boot of the machine's own are refused, with why.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, boolean, list, reference, string};
use isoloom_spec::model::{Boot, BootDevice, Firmware};

use crate::machine::vms;
use crate::server::boot::{Field, HOST_FILES, first_set};

/// Why Proxmox can't boot a machine its own way, if it can't: the first field it has no
/// setting for.
pub(super) fn refusal(env: &Environment) -> Option<String> {
    vms(env).find_map(|(name, mp)| {
        let b = mp.boot.as_ref()?;
        if let Some(field) = first_set(b, HOST_FILES.into_iter().chain([Field::Network])) {
            return Some(format!(
                "machine `{name}` sets `hardware.boot.{field}`: Proxmox boots the VM's disk through its own firmware (`firmware`, `order` and `firmware_format` apply); Vagrant's libvirt provider boots it"
            ));
        }
        if mp.is_windows() && b.firmware.is_some() {
            return Some(format!("machine `{name}` sets `hardware.boot.firmware`: a Windows machine boots its template's UEFI on Proxmox"));
        }
        if b.order.contains(&BootDevice::Cdrom) {
            return Some(format!("machine `{name}` boots from `cdrom`: its Proxmox VM has no CD-ROM drive"));
        }
        (b.firmware_format.is_some() && b.firmware != Some(Firmware::Uefi)).then(|| {
            format!("machine `{name}` sets `hardware.boot.firmware_format`: on Proxmox it is the EFI disk's, with `firmware: uefi`")
        })
    })
}

/// The VM's firmware and boot order, when the machine sets them.
pub(super) fn add(mut vm: BlockBuilder, boot: Option<&Boot>) -> BlockBuilder {
    let Some(b) = boot else { return vm };
    match b.firmware {
        Some(Firmware::Bios) => vm = vm.add_attribute(("bios", string("seabios"))),
        Some(Firmware::Uefi) => {
            let format = b.firmware_format.map_or("raw", |f| f.id());
            vm = vm.add_attribute(("bios", string("ovmf"))).add_block(
                Block::builder("efi_disk")
                    .add_attribute(("datastore_id", reference("var.datastore")))
                    .add_attribute(("file_format", string(format)))
                    .add_attribute(("type", string("4m")))
                    .add_attribute(("pre_enrolled_keys", boolean(false)))
                    .build(),
            );
        }
        None => {}
    }
    if !b.order.is_empty() {
        let devices = b.order.iter().map(|d| string(if *d == BootDevice::Network { "net0" } else { "virtio0" }));
        vm = vm.add_attribute(("boot_order", list(devices)));
    }
    vm
}
