//! The Proxmox output's cells for a machine's hardware: the server's GPUs passed through by
//! name, nested virtualization from the server's own CPU; no host devices.

use crate::spec::status::PROBE;
use crate::spec::status::Status::{self, Done, NotApplicable, Planned};

/// What Proxmox does with every device field: refuses it, with the reason.
const NO_DEVICE: Status = Planned {
    note: "a VM has its own virtual devices: refused with the reason",
};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "machines.*.nested_virtualization",
        Done {
            how: "the VM's CPU is the server's (`cpu.type = host`), with its virtualization extensions where the server's KVM allows nesting",
            proof: PROBE,
        },
    ),
    (
        "machines.*.gpus.count",
        Planned {
            note: "Proxmox can't pick GPUs by count: refused with the reason (name them in `ids`)",
        },
    ),
    (
        "machines.*.gpus.ids",
        Done {
            how: "a `hostpci` PCIe device per id (a PCI address, or a resource mapping), on a q35 machine",
            proof: PROBE,
        },
    ),
    (
        "machines.*.gpus.driver",
        NotApplicable {
            why: "the whole PCI device is passed through: the guest's own driver runs it",
        },
    ),
    (
        "machines.*.gpus.capabilities",
        NotApplicable {
            why: "a VM gets the whole GPU, so every capability",
        },
    ),
    (
        "machines.*.gpus.options",
        NotApplicable {
            why: "Docker's GPU driver options: a VM has none",
        },
    ),
    ("machines.*.devices[].host", NO_DEVICE),
    ("machines.*.devices[].guest", NO_DEVICE),
    ("machines.*.devices[].permissions", NO_DEVICE),
];
