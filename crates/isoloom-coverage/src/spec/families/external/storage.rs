//! The external output's cells for where machines keep data: volumes kept past `down` or existing already, a
//! folder of a shared volume, sized tmpfs mounts and data disks.

use crate::spec::status::Status;
use crate::spec::status::Status::{NotApplicable, Planned};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "volumes.*.persistent",
        Planned {
            note: "an existing host keeps its own data: nothing to keep or remove",
        },
    ),
    (
        "volumes.*.external",
        Planned {
            note: "an existing volume by name is Docker's and Kubernetes'",
        },
    ),
    (
        "machines.*.volumes.*.path",
        Planned {
            note: "the path created on the host",
        },
    ),
    (
        "machines.*.volumes.*.subpath",
        Planned {
            note: "an NFS export on the owner, mounts on the others",
        },
    ),
    (
        "machines.*.tmpfs[].path",
        NotApplicable {
            why: "the host mounts its own",
        },
    ),
    (
        "machines.*.tmpfs[].size",
        NotApplicable {
            why: "the host mounts its own",
        },
    ),
    (
        "machines.*.tmpfs[].mode",
        NotApplicable {
            why: "the host mounts its own",
        },
    ),
    (
        "machines.*.disks.*.size_gb",
        Planned {
            note: "Isoloom attaches nothing to an existing host",
        },
    ),
    (
        "machines.*.disks.*.fs",
        Planned {
            note: "Isoloom attaches nothing to an existing host",
        },
    ),
    (
        "machines.*.disks.*.mount",
        Planned {
            note: "Isoloom attaches nothing to an existing host",
        },
    ),
];
