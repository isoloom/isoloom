//! The Proxmox output's cells for where machines keep data: volumes kept past `down` or existing already, a
//! folder of a shared volume, sized tmpfs mounts and data disks.

use crate::spec::status::Status::{Done, NotApplicable, Planned};
use crate::spec::status::{PROBE, Status};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "volumes.*.persistent",
        Planned {
            note: "a VM's storage goes with it: `down` destroys its disks (Docker and Kubernetes keep a volume)",
        },
    ),
    (
        "volumes.*.external",
        Planned {
            note: "an existing volume by name is Docker's and Kubernetes': a VM has none to mount",
        },
    ),
    (
        "machines.*.volumes.*.path",
        Done {
            how: "the path, created or mounted",
            proof: PROBE,
        },
    ),
    (
        "machines.*.volumes.*.subpath",
        Done {
            how: "the owner's folder, created and served over NFS, mounted at the path",
            proof: PROBE,
        },
    ),
    (
        "machines.*.tmpfs[].path",
        NotApplicable {
            why: "a VM mounts tmpfs in its own provisioning (fstab)",
        },
    ),
    (
        "machines.*.tmpfs[].size",
        NotApplicable {
            why: "a VM mounts tmpfs in its own provisioning (fstab)",
        },
    ),
    (
        "machines.*.tmpfs[].mode",
        NotApplicable {
            why: "a VM mounts tmpfs in its own provisioning (fstab)",
        },
    ),
    (
        "machines.*.disks.*.size_gb",
        Done {
            how: "an empty disk of that size, its serial its name; cloud-init partitions, formats and mounts it",
            proof: PROBE,
        },
    ),
    (
        "machines.*.disks.*.fs",
        Done {
            how: "formatted once with its name as the label (a raw disk is left as it is)",
            proof: PROBE,
        },
    ),
    (
        "machines.*.disks.*.mount",
        Done {
            how: "mounted at boot from /etc/fstab (a drive letter on Windows)",
            proof: PROBE,
        },
    ),
];
