//! The tables' data: for each rendering, what it does with each field of the spec. Data, not
//! code: one entry a field, split by section so no table grows past a screenful of files.

mod cloud_services;
mod cloud_vm;
mod compose;
mod external;
pub mod fields;
mod ipv6;
mod kubernetes;
mod machine_files;
mod outside;
mod process;
mod proxmox;
pub mod shared;
mod vagrant;

use super::output::Family;
use super::status::Status;

/// Cloud services aren't machines: a field cloud services don't list means nothing to them.
const NOT_A_SERVICE: Status = Status::NotApplicable {
    why: "cloud services aren't machines",
};

/// A family's own entries, section by section.
pub fn parts(family: Family) -> &'static [&'static [(&'static str, Status)]] {
    match family {
        Family::Compose => compose::PARTS,
        Family::Kubernetes => kubernetes::PARTS,
        Family::Vagrant => vagrant::PARTS,
        Family::Proxmox => proxmox::PARTS,
        Family::CloudVm => cloud_vm::PARTS,
        Family::External => external::PARTS,
        Family::CloudServices => &[cloud_services::CELLS],
    }
}

/// What `family` does with the field at `path`: its own entry, else the one every output
/// shares, else (cloud services only) not applicable. `None` when nothing states it, which the
/// tests reject.
pub fn status(family: Family, path: &str) -> Option<Status> {
    let own = parts(family).iter().flat_map(|p| p.iter()).find(|(p, _)| *p == path);
    let shared = || shared::CELLS.iter().find(|(p, _)| *p == path);
    match own.or_else(shared) {
        Some((_, s)) => Some(*s),
        None if family == Family::CloudServices => Some(NOT_A_SERVICE),
        None => None,
    }
}
