//! The Kubernetes output's cells for a machine's hardware: GPUs as the device plugin's resource,
//! which the scheduler places; no host devices, no VMs inside a pod.

use crate::spec::status::PROBE;
use crate::spec::status::Status::{self, Done, Partial, Planned};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "machines.*.nested_virtualization",
        Planned {
            note: "a pod runs no VMs: the machine runs as a VM on the VM targets",
        },
    ),
    (
        "machines.*.gpus.count",
        Done {
            how: "the device plugin's resource in the container's limits (`nvidia.com/gpu: <count>`)",
            proof: PROBE,
        },
    ),
    (
        "machines.*.gpus.ids",
        Planned {
            note: "the scheduler picks the GPUs: refused with the reason",
        },
    ),
    (
        "machines.*.gpus.driver",
        Partial {
            how: "the resource's vendor: `nvidia.com/gpu`, `amd.com/gpu`, `gpu.intel.com/i915`",
            gap: "another driver has no resource Isoloom knows: refused",
            proof: PROBE,
        },
    ),
    (
        "machines.*.gpus.capabilities",
        Done {
            how: "NVIDIA_DRIVER_CAPABILITIES in the container's environment",
            proof: PROBE,
        },
    ),
    (
        "machines.*.gpus.options",
        Planned {
            note: "Docker's GPU driver options have no place in a pod: refused with the reason",
        },
    ),
    (
        "machines.*.devices[].host",
        Planned {
            note: "a pod reaches a node's devices only through a device plugin: refused with the reason",
        },
    ),
    (
        "machines.*.devices[].guest",
        Planned {
            note: "with `devices`: refused with the reason",
        },
    ),
    (
        "machines.*.devices[].permissions",
        Planned {
            note: "with `devices`: refused with the reason",
        },
    ),
];
