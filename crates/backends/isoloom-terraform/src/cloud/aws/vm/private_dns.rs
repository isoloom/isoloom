//! The private DNS zone on AWS: a Route 53 private hosted zone associated with the VPC, and its
//! A records. The VPC resolves through Route 53 (`enable_dns_support`, `enable_dns_hostnames`:
//! see `network`).

use crate::hcl_builder::{Block, Module, number, string};
use isoloom_plan::Environment;

use super::resources as res;
use crate::cloud::vm::private_dns::{TTL, address, records};

/// The zone and its records, when the environment has one.
pub(super) fn write(env: &Environment, module: &mut Module) {
    let Some(zone) = &env.private_dns else { return };
    module.commented(
        "The environment's names, served by Route 53 inside the VPC (`dns.private`).",
        res::route53_zone()
            .block()
            .add_attribute(("name", string(zone.domain.as_str())))
            .add_block(Block::builder("vpc").add_attribute(("vpc_id", res::vpc().attr("id"))).build())
            .build(),
    );
    for (id, r) in records(zone) {
        module.block(
            res::route53_record(&id)
                .block()
                .add_attribute(("zone_id", res::route53_zone().attr("zone_id")))
                .add_attribute(("name", string(r.name.as_str())))
                .add_attribute(("type", string("A")))
                .add_attribute(("ttl", number(TTL)))
                .add_attribute(("records", address(r)))
                .build(),
        );
    }
}
