//! The Kubernetes output's cells for where machines keep data: volumes kept past `down` or existing already, a
//! folder of a shared volume, sized tmpfs mounts and data disks.

use crate::spec::status::Status::{Done, Partial, Planned};
use crate::spec::status::{PROBE, Status};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "volumes.*.persistent",
        Done {
            how: "a claim labelled `isoloom.persistent` (its namespace too) that `down` leaves, and that Helm keeps (`helm.sh/resource-policy: keep`)",
            proof: PROBE,
        },
    ),
    (
        "volumes.*.external",
        Done {
            how: "the existing claim's name, with no claim of its own",
            proof: PROBE,
        },
    ),
    (
        "machines.*.volumes.*.path",
        Done {
            how: "the mount's path",
            proof: PROBE,
        },
    ),
    (
        "machines.*.volumes.*.subpath",
        Done {
            how: "`subPath` on the mount",
            proof: PROBE,
        },
    ),
    (
        "machines.*.tmpfs[].path",
        Done {
            how: "a Memory emptyDir",
            proof: PROBE,
        },
    ),
    (
        "machines.*.tmpfs[].size",
        Done {
            how: "the emptyDir's `sizeLimit`",
            proof: PROBE,
        },
    ),
    (
        "machines.*.tmpfs[].mode",
        Planned {
            note: "Kubernetes' memory volumes have no mode",
        },
    ),
    (
        "machines.*.disks.*.size_gb",
        Done {
            how: "a claim of that size",
            proof: PROBE,
        },
    ),
    (
        "machines.*.disks.*.fs",
        Partial {
            how: "`volumeMode: Block` for a raw disk (a device at /dev/isoloom-<disk>), `Filesystem` otherwise",
            gap: "the filesystem's type: the storage class picks it",
            proof: PROBE,
        },
    ),
    (
        "machines.*.disks.*.mount",
        Done {
            how: "the claim mounted at the path",
            proof: PROBE,
        },
    ),
];
