//! The Kubernetes manifests, Helm chart and Kustomize overlay (`kubernetes`): what it does with each field of the spec, by section. A field a section
//! doesn't list is one every output treats alike ([`super::shared`]).

mod environment;
mod hardware;
mod machines;

use crate::spec::status::Status;

/// The Kubernetes output's cells, section by section.
pub const PARTS: &[&[(&str, Status)]] = &[
    environment::CELLS,
    machines::CELLS,
    super::ipv6::KUBERNETES,
    super::outside::KUBERNETES,
    super::machine_files::KUBERNETES,
    super::process::KUBERNETES,
    super::own_image::CONTAINER_IMAGES,
    super::own_image::CONTAINER_BOOT,
    hardware::CELLS,
];
