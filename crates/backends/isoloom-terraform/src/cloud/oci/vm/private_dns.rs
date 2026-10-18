//! The private DNS zone on Oracle Cloud: a private view holding the zone and its records,
//! attached to the VCN's own resolver.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Module, number, reference, string};

use crate::cloud::oci::owned;
use crate::cloud::vm::private_dns::{TTL, records};
use crate::module::resource;

/// The zone, its records and the resolver's view, when the environment has one.
pub(super) fn write(env: &Environment, m: &mut Module) {
    let Some(zone) = &env.private_dns else { return };
    m.commented(
        "The environment's names, served by the VCN's resolver from a private view (`dns.private`).",
        owned(resource("oci_dns_view", "private"))
            .add_attribute(("scope", string("PRIVATE")))
            .add_attribute(("display_name", reference("local.name")))
            .build(),
    );
    m.block(
        owned(resource("oci_dns_zone", "private"))
            .add_attribute(("name", string(zone.domain.as_str())))
            .add_attribute(("zone_type", string("PRIMARY")))
            .add_attribute(("scope", string("PRIVATE")))
            .add_attribute(("view_id", reference("oci_dns_view.private.id")))
            .build(),
    );
    for (id, r) in records(zone) {
        let item = Block::builder("items")
            .add_attribute(("domain", string(r.name.as_str())))
            .add_attribute(("rtype", string("A")))
            .add_attribute(("rdata", string(r.address.to_string())))
            .add_attribute(("ttl", number(TTL)))
            .build();
        // The zone gives its scope and view: an rrset's own are ignored.
        m.block(
            resource("oci_dns_rrset", &id)
                .add_attribute(("zone_name_or_id", reference("oci_dns_zone.private.id")))
                .add_attribute(("domain", string(r.name.as_str())))
                .add_attribute(("rtype", string("A")))
                .add_block(item)
                .build(),
        );
    }
    m.block(
        Block::builder("data")
            .add_label("oci_core_vcn_dns_resolver_association")
            .add_label("env")
            .add_attribute(("vcn_id", reference("oci_core_vcn.env.id")))
            .build(),
    );
    let attached = Block::builder("attached_views")
        .add_attribute(("view_id", reference("oci_dns_view.private.id")))
        .build();
    m.block(
        resource("oci_dns_resolver", "env")
            .add_attribute(("resolver_id", reference("data.oci_core_vcn_dns_resolver_association.env.dns_resolver_id")))
            .add_attribute(("scope", string("PRIVATE")))
            .add_block(attached)
            .build(),
    );
}
