//! A machine's published ports, on its own primary VNIC only: a network security group of its
//! own, one rule per port, so a port opens on the machine that publishes it and no other.

use isoloom_plan::MachinePlan;
use isoloom_render::hcl_builder::{Block, Expression, Module, number, reference, string};

use super::{named, resource};
use crate::vm::cloud::name_with;
use crate::vm::res;

/// A machine's own network security group, with a rule per port it publishes, so a port opens
/// on the machine that publishes it and no other. Returns the group's id, or `None` when the
/// machine publishes nothing.
pub fn group(name: &str, mp: &MachinePlan, m: &mut Module) -> Option<Expression> {
    if mp.ports.is_empty() {
        return None;
    }
    let id = res(name);
    m.commented(
        &format!("The ports `{name}` publishes, from allowed_cidr: on its VNIC only."),
        named(resource("oci_core_network_security_group", &id))
            .add_attribute(("vcn_id", reference("oci_core_vcn.env.id")))
            .add_attribute(("display_name", name_with(&format!("-{name}"))))
            .build(),
    );
    let mut ports: Vec<(bool, u16)> = mp.ports.iter().map(|p| (p.protocol.is_tcp(), p.publish)).collect();
    ports.sort_unstable_by_key(|&(tcp, port)| (!tcp, port));
    ports.dedup();
    for (tcp, port) in ports {
        m.block(published_rule(&id, tcp, port));
    }
    Some(reference(&format!("oci_core_network_security_group.{id}.id")))
}

/// The rule opening one published port (TCP is 6, UDP 17) of group `id`.
fn published_rule(id: &str, tcp: bool, port: u16) -> Block {
    let (protocol, options, label) = if tcp { ("6", "tcp_options", "tcp") } else { ("17", "udp_options", "udp") };
    let range = Block::builder("destination_port_range")
        .add_attribute(("min", number(u64::from(port))))
        .add_attribute(("max", number(u64::from(port))))
        .build();
    resource("oci_core_network_security_group_security_rule", &format!("{id}_{label}_{port}"))
        .add_attribute(("network_security_group_id", reference(&format!("oci_core_network_security_group.{id}.id"))))
        .add_attribute(("description", string("published")))
        .add_attribute(("direction", string("INGRESS")))
        .add_attribute(("protocol", string(protocol)))
        .add_attribute(("source", reference("var.allowed_cidr")))
        .add_attribute(("source_type", string("CIDR_BLOCK")))
        .add_block(Block::builder(options).add_block(range).build())
        .build()
}
