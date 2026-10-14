//! The Vagrantfile (`vagrant`, and the VMs of `hybrid`): what it does with each field of the spec, by section. A field a section
//! doesn't list is one every output treats alike ([`super::shared`]).

mod environment;
mod hardware;
mod machines;

use crate::spec::status::Status;

/// The Vagrant output's cells, section by section.
pub const PARTS: &[&[(&str, Status)]] = &[
    environment::CELLS,
    machines::CELLS,
    super::ipv6::VAGRANT,
    super::outside::VAGRANT,
    super::machine_files::VM,
    super::process::VM,
    super::own_image::VAGRANT_IMAGES,
    super::own_image::VAGRANT_BOOT,
    hardware::CELLS,
];
