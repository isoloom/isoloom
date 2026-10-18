//! The private DNS zone on Azure: a private DNS zone linked to the virtual network, its A records
//! (relative names) and AAAA records on IPv6 networks, and the reverse zones with their PTR
//! records.

use std::net::{Ipv4Addr, Ipv6Addr};

use isoloom_plan::Environment;
use isoloom_plan::private_dns::{PrivateDns, Record};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, list, number, reference, string};

use crate::cloud::vm::private_dns::{TTL, address, records};
use crate::module::{ident, resource};

/// The zone and its records, when the environment has one.
pub(super) fn write(env: &Environment, m: &mut Module) {
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
        m.block(record("azurerm_private_dns_a_record", &id, r, address(r)));
        // A machine on an IPv6 network first: its IPv6 address there too.
        if let Some(v6) = address6(env, r) {
            m.block(record("azurerm_private_dns_aaaa_record", &id, r, list([string(v6.to_string())])));
        }
    }
    reverse_zones(zone, m);
}

/// One of the zone's records (relative name), of `kind`, with its values.
fn record(kind: &str, id: &str, r: &Record, values: Expression) -> Block {
    tagged(
        resource(kind, id)
            .add_attribute(("name", string(r.label.as_str())))
            .add_attribute(("private_dns_zone_id", reference("azurerm_private_dns_zone.private.id")))
            .add_attribute(("ttl", number(TTL)))
            .add_attribute(("records", values)),
    )
    .build()
}

/// Tagged as the environment's.
fn tagged(b: BlockBuilder) -> BlockBuilder {
    b.add_attribute(("tags", reference("local.tags")))
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

/// The reverse names: a reverse zone per network block (each /24 holding a machine's
/// address) linked to the virtual network, and a PTR record per machine at its first address,
/// naming the machine (its aliases point at it, but an address names one machine).
fn reverse_zones(zone: &PrivateDns, m: &mut Module) {
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
