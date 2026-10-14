//! The cloud VM core's cells for a machine's hardware: a count of GPUs picks the cloud's GPU
//! instance type; no nested virtualization and no host devices.

use crate::spec::status::PROBE;
use crate::spec::status::Status::{self, NotApplicable, Partial, Planned};

/// What a cloud does with every device field: refuses it, with the reason.
const NO_DEVICE: Status = Planned {
    note: "a VM has its own virtual devices: refused with the reason",
};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "machines.*.nested_virtualization",
        Partial {
            how: "a size that nests and the cloud's switch: AWS m8i with `cpu_options.nested_virtualization`, Google Cloud n2 with `enable_nested_virtualization`, Oracle Cloud's AMD shape with `are_virtual_instructions_enabled`",
            gap: "Azure, DigitalOcean and Linode refuse it for now; not with GPUs",
            proof: PROBE,
        },
    ),
    (
        "machines.*.gpus.count",
        Partial {
            how: "the cloud's smallest GPU instance type with as many: AWS g4dn (T4), Azure NCasT4_v3 (T4), Google Cloud g2 (L4), DigitalOcean H100, Linode RTX 6000",
            gap: "the guest's NVIDIA driver comes from the machine's own provisioning (the stock images have none); Oracle Cloud refuses GPUs for now",
            proof: PROBE,
        },
    ),
    (
        "machines.*.gpus.ids",
        Planned {
            note: "a cloud VM's GPUs come with its instance type: refused with the reason",
        },
    ),
    (
        "machines.*.gpus.driver",
        Partial {
            how: "`nvidia`: the GPUs the clouds' types carry",
            gap: "another driver: refused",
            proof: PROBE,
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
