//! Every output's cells for a shared volume as a folder of the Proxmox server
//! (`volumes.*.virtiofs`), in one place: the Proxmox module shares it into each VM, every other
//! output shares the volume as it always does and has no use for the setting.

use crate::spec::status::PROBE;
use crate::spec::status::Status::{self, Done, NotApplicable};

/// The setting's fields, each meaning nothing to an output that isn't Proxmox's.
const fn elsewhere(why: &'static str) -> [(&'static str, Status); 5] {
    let s = NotApplicable { why };
    [
        ("volumes.*.virtiofs.mapping", s),
        ("volumes.*.virtiofs.cache", s),
        ("volumes.*.virtiofs.direct_io", s),
        ("volumes.*.virtiofs.xattr", s),
        ("volumes.*.virtiofs.acl", s),
    ]
}

/// The outputs that share a volume their own way: a Docker volume, a claim, NFS between VMs.
pub const NOT_PROXMOX: &[(&str, Status)] = &elsewhere("Proxmox only: elsewhere the volume is shared as usual (a Docker volume, a claim, NFS from its owner)");

/// The ESXi module's: a standalone host has no folder to share into a VM.
pub const ESXI: &[(&str, Status)] = &elsewhere("Proxmox only: ESXi shares the volume over NFS from its owner (a standalone host has no virtiofs)");

/// The Proxmox module's (not run on a real host).
pub const PROXMOX: &[(&str, Status)] = &[
    (
        "volumes.*.virtiofs.mapping",
        Done {
            how: "a `virtiofs` device on each VM that mounts the volume, mounted by its tag from /etc/fstab; the named directory mapping, else one the module creates (`proxmox_hardware_mapping_dir`, `<virtiofs_root>/<environment>/<volume>` on the node) (not run on a real host)",
            proof: PROBE,
        },
    ),
    (
        "volumes.*.virtiofs.cache",
        Done {
            how: "`virtiofs.cache` on each VM's device (`auto` by default) (not run on a real host)",
            proof: PROBE,
        },
    ),
    (
        "volumes.*.virtiofs.direct_io",
        Done {
            how: "`virtiofs.direct_io` on each VM's device (not run on a real host)",
            proof: PROBE,
        },
    ),
    (
        "volumes.*.virtiofs.xattr",
        Done {
            how: "`virtiofs.expose_xattr` on each VM's device (not run on a real host)",
            proof: PROBE,
        },
    ),
    (
        "volumes.*.virtiofs.acl",
        Done {
            how: "`virtiofs.expose_acl`, with `expose_xattr`, on each VM's device (not run on a real host)",
            proof: PROBE,
        },
    ),
];

#[cfg(test)]
mod tests {
    #[test]
    fn every_output_states_the_same_fields() {
        let paths = |cells: &[(&str, super::Status)]| cells.iter().map(|(p, _)| p.to_string()).collect::<Vec<_>>();
        assert_eq!(paths(super::NOT_PROXMOX), paths(super::PROXMOX));
        assert_eq!(paths(super::ESXI), paths(super::PROXMOX));
    }

    #[test]
    fn the_cells_are_the_helpers_at_run_time() {
        // A const fn runs at compile time only: called here, its lines count as run.
        let why = "Proxmox only: ESXi shares the volume over NFS from its owner (a standalone host has no virtiofs)";
        assert_eq!(&super::elsewhere(why)[..], super::ESXI);
    }
}
