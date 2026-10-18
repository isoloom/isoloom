//! The private DNS zone on AWS: a Route 53 private hosted zone associated with the VPC, and its
//! A records. The VPC resolves through Route 53 (`enable_dns_support`, `enable_dns_hostnames`:
//! see `network`).

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Module, number, reference, string};

use crate::cloud::vm::private_dns::{TTL, address, records};
use crate::module::resource;

/// The zone and its records, when the environment has one.
pub(super) fn write(env: &Environment, m: &mut Module) {
    let Some(zone) = &env.private_dns else { return };
    m.commented(
        "The environment's names, served by Route 53 inside the VPC (`dns.private`).",
        resource("aws_route53_zone", "private")
            .add_attribute(("name", string(zone.domain.as_str())))
            .add_block(Block::builder("vpc").add_attribute(("vpc_id", reference("aws_vpc.env.id"))).build())
            .build(),
    );
    for (id, r) in records(zone) {
        m.block(
            resource("aws_route53_record", &id)
                .add_attribute(("zone_id", reference("aws_route53_zone.private.zone_id")))
                .add_attribute(("name", string(r.name.as_str())))
                .add_attribute(("type", string("A")))
                .add_attribute(("ttl", number(TTL)))
                .add_attribute(("records", address(r)))
                .build(),
        );
    }
}
