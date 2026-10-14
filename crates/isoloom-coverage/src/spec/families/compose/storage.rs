//! The Docker Compose output's cells for where machines keep data: volumes kept past `down` or existing already, a
//! folder of a shared volume, sized tmpfs mounts and data disks.

use crate::spec::status::Status::{Done, NotApplicable};
use crate::spec::status::{PROBE, Status};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "volumes.*.persistent",
        Done {
            how: "a named volume labelled `isoloom.persistent`: `down` keeps it, `down --volumes` removes it (refused on a Docker host Isoloom creates)",
            proof: PROBE,
        },
    ),
    (
        "volumes.*.external",
        Done {
            how: "`external: true` with the volume's name: never created or removed (refused on a Docker host Isoloom creates)",
            proof: PROBE,
        },
    ),
    (
        "machines.*.volumes.*.path",
        Done {
            how: "the mount's path, as the short form's",
            proof: PROBE,
        },
    ),
    (
        "machines.*.volumes.*.subpath",
        Done {
            how: "a long-form mount of the folder (`volume.subpath`), created first by a one-shot job",
            proof: PROBE,
        },
    ),
    (
        "machines.*.tmpfs[].path",
        Done {
            how: "a memory-backed mount",
            proof: PROBE,
        },
    ),
    (
        "machines.*.tmpfs[].size",
        Done {
            how: "`tmpfs.size` on a long-form mount",
            proof: PROBE,
        },
    ),
    (
        "machines.*.tmpfs[].mode",
        Done {
            how: "`tmpfs.mode` on a long-form mount",
            proof: PROBE,
        },
    ),
    (
        "machines.*.disks.*.size_gb",
        NotApplicable {
            why: "a container's disk is a named volume on the host's disk: no size of its own",
        },
    ),
    (
        "machines.*.disks.*.fs",
        NotApplicable {
            why: "a container's disk is a named volume on the host's filesystem (a raw disk is refused: no block device)",
        },
    ),
    (
        "machines.*.disks.*.mount",
        Done {
            how: "a named volume (`<machine>-<disk>`) at the path",
            proof: PROBE,
        },
    ),
];
