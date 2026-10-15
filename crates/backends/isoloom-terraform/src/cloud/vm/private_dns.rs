//! The private DNS zone (`dns.private`) on the clouds that have one: the zone, bound to the
//! environment's network, and one A record per name of the plan's zone (each machine's name and
//! aliases, at its first address). DigitalOcean and Linode have no private DNS: their VMs'
//! hosts files carry the names, as on every VM target.

use std::net::{Ipv4Addr, Ipv6Addr};

use isoloom_plan::Environment;
use isoloom_plan::private_dns::{PrivateDns, Record};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, boolean, list, number, reference, string};

use crate::cloud::instance::name_with;
use crate::module::{ident, resource};

/// How long a resolver keeps an answer: short, since addresses only change when a machine is
/// rebuilt.
const TTL: u64 = 60;

/// The zone's records, each with its Terraform name (unique: the full name made a label).
fn records(zone: &PrivateDns) -> impl Iterator<Item = (String, &Record)> {
    zone.records.iter().map(|r| (ident(&r.name), r))
}

/// One address, as a record's list of values.
fn address(r: &Record) -> Expression {
    list([string(r.address.to_string())])
}

/// AWS: a Route 53 private hosted zone associated with the VPC, and its A records. The VPC
/// resolves through Route 53 (`enable_dns_support`, `enable_dns_hostnames`: see `network`).
pub(crate) fn aws(env: &Environment, m: &mut Module) {
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

/// Tagged as the environment's.
fn tagged(b: BlockBuilder) -> BlockBuilder {
    b.add_attribute(("tags", reference("local.tags")))
}

/// Azure: a private DNS zone linked to the virtual network, its A records (relative names) and
/// AAAA records on IPv6 networks, and the reverse zones with their PTR records.
pub(crate) fn azure(env: &Environment, m: &mut Module) {
    let Some(zone) = &env.private_dns else { return };
    m.commented(
        "The environment's names, served by Azure DNS inside the virtual network (`dns.private`).",
        tagged(
            resource("azurerm_private_dns_zone", "private")
                .add_attribute(("name", string(zone.domain.as_str())))
                .add_attribute(("resource_group_name", reference("azurerm_resource_group.env.name"))),
        )
        .build(),
    );
    m.block(
        tagged(
            resource("azurerm_private_dns_zone_virtual_network_link", "private")
                .add_attribute(("name", reference("local.name")))
                .add_attribute(("private_dns_zone_id", reference("azurerm_private_dns_zone.private.id")))
                .add_attribute(("virtual_network_id", reference("azurerm_virtual_network.env.id"))),
        )
        .build(),
    );
    for (id, r) in records(zone) {
        m.block(azure_record("azurerm_private_dns_a_record", &id, r, address(r)));
        // A machine on an IPv6 network first: its IPv6 address there too.
        if let Some(v6) = address6(env, r) {
            m.block(azure_record("azurerm_private_dns_aaaa_record", &id, r, list([string(v6.to_string())])));
        }
    }
    azure_reverse(zone, m);
}

/// One of the zone's Azure records (relative name), of `kind`, with its values.
fn azure_record(kind: &str, id: &str, r: &Record, values: Expression) -> Block {
    tagged(
        resource(kind, id)
            .add_attribute(("name", string(r.label.as_str())))
            .add_attribute(("private_dns_zone_id", reference("azurerm_private_dns_zone.private.id")))
            .add_attribute(("ttl", number(TTL)))
            .add_attribute(("records", values)),
    )
    .build()
}

/// The IPv6 address of a record's machine on its first network, when that network is IPv6 too.
fn address6(env: &Environment, r: &Record) -> Option<Ipv6Addr> {
    let mp = env.machines.get(&r.machine)?;
    let first = mp.addresses.keys().next()?;
    mp.addresses6.get(first).copied()
}

/// The reverse zone holding an address: its /24 (`30.20.10.in-addr.arpa` for 10.20.30.x), and
/// the address's name in it (its last octet).
fn reverse(a: Ipv4Addr) -> (String, String) {
    let o = a.octets();
    (format!("{}.{}.{}.in-addr.arpa", o[2], o[1], o[0]), o[3].to_string())
}

/// Azure's reverse names: a reverse zone per network block (each /24 holding a machine's
/// address) linked to the virtual network, and a PTR record per machine at its first address,
/// naming the machine (its aliases point at it, but an address names one machine).
fn azure_reverse(zone: &PrivateDns, m: &mut Module) {
    let mut zones: Vec<String> = Vec::new();
    for r in zone.records.iter().filter(|r| r.label == r.machine) {
        let (name, label) = reverse(r.address);
        let zid = format!("reverse_{}", ident(name.trim_end_matches(".in-addr.arpa")));
        if !zones.contains(&name) {
            m.commented(
                &format!("The reverse names of {name}: each machine's name at its address."),
                tagged(
                    resource("azurerm_private_dns_zone", &zid)
                        .add_attribute(("name", string(name.as_str())))
                        .add_attribute(("resource_group_name", reference("azurerm_resource_group.env.name"))),
                )
                .build(),
            );
            m.block(
                tagged(
                    resource("azurerm_private_dns_zone_virtual_network_link", &zid)
                        .add_attribute(("name", reference("local.name")))
                        .add_attribute(("private_dns_zone_id", reference(&format!("azurerm_private_dns_zone.{zid}.id"))))
                        .add_attribute(("virtual_network_id", reference("azurerm_virtual_network.env.id"))),
                )
                .build(),
            );
            zones.push(name);
        }
        m.block(
            tagged(
                resource("azurerm_private_dns_ptr_record", &ident(&r.name))
                    .add_attribute(("name", string(label)))
                    .add_attribute(("private_dns_zone_id", reference(&format!("azurerm_private_dns_zone.{zid}.id"))))
                    .add_attribute(("ttl", number(TTL)))
                    .add_attribute(("records", list([string(r.name.as_str())]))),
            )
            .build(),
        );
    }
}

/// Google Cloud: the Cloud DNS API (in a project of its own), a private managed zone visible
/// from every network's VPC network, and its A records (names end with a dot).
pub(crate) fn gcp(env: &Environment, own_project: Expression, m: &mut Module) {
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

/// Oracle Cloud: a private view holding the zone and its records, attached to the VCN's own
/// resolver.
pub(crate) fn oci(env: &Environment, m: &mut Module) {
    let Some(zone) = &env.private_dns else { return };
    let compartment = || reference("var.compartment_id");
    m.commented(
        "The environment's names, served by the VCN's resolver from a private view (`dns.private`).",
        resource("oci_dns_view", "private")
            .add_attribute(("compartment_id", compartment()))
            .add_attribute(("scope", string("PRIVATE")))
            .add_attribute(("display_name", reference("local.name")))
            .build(),
    );
    m.block(
        resource("oci_dns_zone", "private")
            .add_attribute(("compartment_id", compartment()))
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
