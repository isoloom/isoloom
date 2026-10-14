//! The Compose output's cells for a machine's hardware: GPUs and the host's devices, from the
//! user's own Docker host. A VM Isoloom creates to run the file (`docker-vm`, `cloud-docker`,
//! `hybrid`) has neither, and its backend refuses them.

use crate::spec::status::PROBE;
use crate::spec::status::Status::{self, Done, Planned};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "machines.*.nested_virtualization",
        Planned {
            note: "a container runs no VMs: the machine runs as a VM on the VM targets",
        },
    ),
    (
        "machines.*.gpus.count",
        Done {
            how: "a GPU request of that count (`deploy.resources.reservations.devices`)",
            proof: PROBE,
        },
    ),
    (
        "machines.*.gpus.ids",
        Done {
            how: "the request's `device_ids`",
            proof: PROBE,
        },
    ),
    (
        "machines.*.gpus.driver",
        Done {
            how: "the request's `driver`",
            proof: PROBE,
        },
    ),
    (
        "machines.*.gpus.capabilities",
        Done {
            how: "the request's `capabilities`, after `gpu`",
            proof: PROBE,
        },
    ),
    (
        "machines.*.gpus.options",
        Done {
            how: "the request's `options`",
            proof: PROBE,
        },
    ),
    (
        "machines.*.devices[].host",
        Done {
            how: "`devices` (long form): `source`",
            proof: PROBE,
        },
    ),
    (
        "machines.*.devices[].guest",
        Done {
            how: "`devices` (long form): `target`",
            proof: PROBE,
        },
    ),
    (
        "machines.*.devices[].permissions",
        Done {
            how: "`devices` (long form): `permissions`",
            proof: PROBE,
        },
    ),
];
