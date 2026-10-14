//! The Compose file (`docker`, `hosted`, `docker-vm`, `cloud-docker`, and the containers of `hybrid`): what it does with each field of the spec, by section. A field a section
//! doesn't list is one every output treats alike ([`super::shared`]).

mod environment;
mod machines;

use crate::spec::status::Status;

/// The Docker Compose output's cells, section by section.
pub const PARTS: &[&[(&str, Status)]] = &[environment::CELLS, machines::CELLS];
