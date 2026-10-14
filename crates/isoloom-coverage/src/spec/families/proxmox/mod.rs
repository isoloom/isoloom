//! The Proxmox Terraform module (`proxmox`): what it does with each field of the spec, by section. A field a section
//! doesn't list is one every output treats alike ([`super::shared`]).

mod environment;
mod machines;

use crate::spec::status::Status;

/// The Proxmox output's cells, section by section.
pub const PARTS: &[&[(&str, Status)]] = &[
    environment::CELLS,
    machines::CELLS,
    super::ipv6::PROXMOX,
    super::outside::PROXMOX,
    super::machine_files::VM,
    super::process::VM,
];
