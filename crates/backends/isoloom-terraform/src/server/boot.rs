//! A machine's own boot (`vm.boot`) on a server: Proxmox and ESXi boot the VM's disk through
//! their own firmware, so the fields that boot it from files of the host are refused on both,
//! and each refuses what else it has no setting for (`proxmox::boot`, `esxi::boot`).

use isoloom_spec::model::{Boot, BootDevice};

/// A field of `vm.boot` a server may have no setting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    /// `kernel`, a kernel of the host's.
    Kernel,
    /// `initrd`.
    Initrd,
    /// `cmdline`.
    Cmdline,
    /// `dtb`, a device tree.
    Dtb,
    /// `loader`, a firmware file.
    Loader,
    /// `nvram`, its variables' file.
    Nvram,
    /// `firmware_format`.
    FirmwareFormat,
    /// `order`, when it boots anything but the disk.
    Order,
    /// `network`, a network boot.
    Network,
}

/// The fields that boot the VM from files of the host: no server has a setting for them.
pub(crate) const HOST_FILES: [Field; 6] = [Field::Kernel, Field::Initrd, Field::Cmdline, Field::Dtb, Field::Loader, Field::Nvram];

impl Field {
    /// Its name under `vm.boot`.
    fn name(self) -> &'static str {
        match self {
            Field::Kernel => "kernel",
            Field::Initrd => "initrd",
            Field::Cmdline => "cmdline",
            Field::Dtb => "dtb",
            Field::Loader => "loader",
            Field::Nvram => "nvram",
            Field::FirmwareFormat => "firmware_format",
            Field::Order => "order",
            Field::Network => "network",
        }
    }

    /// Whether the machine's boot sets it.
    fn is_set(self, b: &Boot) -> bool {
        match self {
            Field::Kernel => b.kernel.is_some(),
            Field::Initrd => b.initrd.is_some(),
            Field::Cmdline => b.cmdline.is_some(),
            Field::Dtb => b.dtb.is_some(),
            Field::Loader => b.loader.is_some(),
            Field::Nvram => b.nvram.is_some(),
            Field::FirmwareFormat => b.firmware_format.is_some(),
            Field::Order => b.order.iter().any(|d| *d != BootDevice::Hd),
            Field::Network => b.network.is_some(),
        }
    }
}

/// The first of `fields` the machine's boot sets, by its name.
pub(crate) fn first_set(b: &Boot, fields: impl IntoIterator<Item = Field>) -> Option<&'static str> {
    fields.into_iter().find(|f| f.is_set(b)).map(Field::name)
}
