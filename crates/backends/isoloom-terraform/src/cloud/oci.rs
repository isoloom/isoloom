//! Oracle Cloud: what its `cloud-vm` module ([`vm`]) and its `cloud-docker` module ([`docker`])
//! share: the provider, the compartment, the availability domains, the shape every instance
//! has, the Ubuntu images for it, and the VCN's way to the internet.

pub(crate) mod docker;
pub(crate) mod vm;

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, attr, index, reference, string};

use crate::cloud::instance;

/// The shape every instance has: AMD, sized by its configuration.
pub(crate) const SHAPE: &str = "VM.Standard.E4.Flex";

/// The provider, in the region, and the free-form tags every resource carries (`local.tags`).
pub(crate) fn provider(env: &Environment, m: &mut Module) -> Vec<(&'static str, Expression)> {
    m.block(
        Block::builder("provider")
            .add_label("oci")
            .add_attribute(("region", reference("var.region")))
            .build(),
    );
    vec![("tags", instance::tags_when_set(env))]
}

/// A resource or data source in the compartment.
pub(crate) fn in_compartment(b: BlockBuilder) -> BlockBuilder {
    b.add_attribute(("compartment_id", reference("var.compartment_id")))
}

/// One of the environment's resources: in the compartment, with its tags.
pub(crate) fn owned(b: BlockBuilder) -> BlockBuilder {
    in_compartment(b).add_attribute(("freeform_tags", reference("local.tags")))
}

/// The compartment's availability domains (`data.oci_identity_availability_domains.ads`).
pub(crate) fn availability_domains() -> Block {
    in_compartment(Block::builder("data").add_label("oci_identity_availability_domains").add_label("ads")).build()
}

/// The first availability domain's name: where the instances land.
pub(crate) fn availability_domain() -> Expression {
    attr(index(reference("data.oci_identity_availability_domains.ads.availability_domains"), 0), "name")
}

/// The newest Canonical Ubuntu `version` for the shape, `data.oci_core_images.<name>`.
pub(crate) fn ubuntu_images(name: &str, version: &str) -> Block {
    in_compartment(Block::builder("data").add_label("oci_core_images").add_label(name))
        .add_attribute(("operating_system", string("Canonical Ubuntu")))
        .add_attribute(("operating_system_version", string(version)))
        .add_attribute(("shape", string(SHAPE)))
        .add_attribute(("sort_by", string("TIMECREATED")))
        .add_attribute(("sort_order", string("DESC")))
        .build()
}

/// A route table's way out: the internet through the internet gateway.
pub(crate) fn route_out() -> Block {
    Block::builder("route_rules")
        .add_attribute(("destination", string("0.0.0.0/0")))
        .add_attribute(("network_entity_id", reference("oci_core_internet_gateway.env.id")))
        .build()
}
