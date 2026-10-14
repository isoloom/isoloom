//! The Vagrant output's cells for a machine's hardware: each provider's nested virtualization;
//! no GPU passthrough and no host devices for a local VM.

use crate::spec::status::PROBE;
use crate::spec::status::Status::{self, Partial, Planned};

/// What Vagrant does with every GPU field: refuses it, with the reason.
const NO_GPU: Status = Planned {
    note: "Vagrant passes no GPU to a local VM: refused with the reason",
};

/// What Vagrant does with every device field: refuses it, with the reason.
const NO_DEVICE: Status = Planned {
    note: "a VM has its own virtual devices: refused with the reason",
};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "machines.*.nested_virtualization",
        Partial {
            how: "each provider's switch: VirtualBox `--nested-hw-virt`, VMware and ESXi `vhv.enable`, Parallels `--nested-virt`, libvirt `nested`, Hyper-V `enable_virtualization_extensions`",
            gap: "QEMU and UTM on a Mac have none: `vagrant up` stops there with the reason",
            proof: PROBE,
        },
    ),
    ("machines.*.gpus.count", NO_GPU),
    ("machines.*.gpus.ids", NO_GPU),
    ("machines.*.gpus.driver", NO_GPU),
    ("machines.*.gpus.capabilities", NO_GPU),
    ("machines.*.gpus.options", NO_GPU),
    ("machines.*.devices[].host", NO_DEVICE),
    ("machines.*.devices[].guest", NO_DEVICE),
    ("machines.*.devices[].permissions", NO_DEVICE),
];
