//! The cloud VM Terraform modules (`cloud-vm`, AWS the reference: the other clouds' refusals come from their backends): what it does with each field of the spec, by section. A field a section
//! doesn't list is one every output treats alike ([`super::shared`]).

mod environment;
mod machines;

use crate::spec::status::Status;

/// The cloud VM output's cells, section by section.
pub const PARTS: &[&[(&str, Status)]] = &[
    environment::CELLS,
    machines::CELLS,
    super::ipv6::CLOUD_VM,
    super::outside::CLOUD_VM,
    super::machine_files::VM,
];
