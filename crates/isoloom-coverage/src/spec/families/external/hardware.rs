//! The external output's cells for a machine's hardware: an existing host has the hardware it
//! has.

use crate::spec::status::{EXISTING_HOST, Status};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    ("machines.*.nested_virtualization", EXISTING_HOST),
    ("machines.*.gpus.count", EXISTING_HOST),
    ("machines.*.gpus.ids", EXISTING_HOST),
    ("machines.*.gpus.driver", EXISTING_HOST),
    ("machines.*.gpus.capabilities", EXISTING_HOST),
    ("machines.*.gpus.options", EXISTING_HOST),
    ("machines.*.devices[].host", EXISTING_HOST),
    ("machines.*.devices[].guest", EXISTING_HOST),
    ("machines.*.devices[].permissions", EXISTING_HOST),
];
