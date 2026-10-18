//! The private DNS zone on Google Cloud: the Cloud DNS API (in a project of its own), a private
//! managed zone visible from every network's VPC network, and its A records (names end with a
//! dot).

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Expression, Module, boolean, list, number, reference, string};

use crate::cloud::instance::name_with;
use crate::cloud::vm::private_dns::{TTL, address, records};
use crate::module::{ident, resource};

/// The zone and its records, when the environment has one; the API enabled when the project is
/// the module's own (`own_project`).
pub(super) fn write(env: &Environment, own_project: Expression, m: &mut Module) {
    let Some(zone) = &env.private_dns else { return };
    m.block(
        resource("google_project_service", "dns")
            .add_attribute(("count", own_project))
            .add_attribute(("project", reference("local.project")))
            .add_attribute(("service", string("dns.googleapis.com")))
            .add_attribute(("disable_on_destroy", boolean(false)))
            .build(),
    );
    // Visible from every network's VPC network.
    let networks = env.networks.keys().map(|n| {
        Block::builder("networks")
            .add_attribute(("network_url", reference(&format!("google_compute_network.{}.id", ident(n)))))
            .build()
    });
    m.commented(
        "The environment's names, served by Cloud DNS inside the VPC networks (`dns.private`).",
        resource("google_dns_managed_zone", "private")
            .add_attribute(("project", reference("local.project")))
            .add_attribute(("name", name_with("-private")))
            .add_attribute(("dns_name", string(format!("{}.", zone.domain))))
            .add_attribute(("visibility", string("private")))
            .add_block(Block::builder("private_visibility_config").add_blocks(networks).build())
            .add_attribute(("depends_on", list([reference("google_project_service.dns")])))
            .build(),
    );
    for (id, r) in records(zone) {
        m.block(
            resource("google_dns_record_set", &id)
                .add_attribute(("project", reference("local.project")))
                .add_attribute(("managed_zone", reference("google_dns_managed_zone.private.name")))
                .add_attribute(("name", string(format!("{}.", r.name))))
                .add_attribute(("type", string("A")))
                .add_attribute(("ttl", number(TTL)))
                .add_attribute(("rrdatas", address(r)))
                .build(),
        );
    }
}
