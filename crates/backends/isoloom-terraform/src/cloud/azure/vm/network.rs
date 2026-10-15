//! The resource group, the virtual network and its subnets; each machine's network security
//! group, public IP and interfaces.

use std::net::{Ipv4Addr, Ipv6Addr};

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, boolean, list, number, reference, string};

use crate::cloud::instance::name_with;
use crate::module::{ident, resource};

/// The resource group and the networks: one virtual network, a subnet per network with its
/// exact range, with no default way out when the network is offline.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    m.block(
        resource("azurerm_resource_group", "env")
            .add_attribute(("name", reference("local.name")))
            .add_attribute(("location", reference("var.region")))
            .add_attribute(("tags", reference("local.tags")))
            .build(),
    );
    // A dual-stack network's IPv6 block too: Azure takes unique local blocks.
    let v6 = env.networks.values().filter_map(|n| n.ipv6.as_ref()).map(|v| string(v.cidr.as_str()));
    let spaces = env.networks.values().map(|n| string(n.cidr.as_str())).chain(v6);
    m.commented(
        "The environment's networks: one virtual network, a subnet per network with its exact range.",
        placed("azurerm_virtual_network", "env", reference("local.name"))
            .add_attribute(("address_space", list(spaces)))
            .build(),
    );
    for (net, p) in &env.networks {
        m.block(
            resource("azurerm_subnet", &ident(net))
                .add_attribute(("name", string(net.as_str())))
                .add_attribute(("resource_group_name", reference("azurerm_resource_group.env.name")))
                .add_attribute(("virtual_network_name", reference("azurerm_virtual_network.env.name")))
                .add_attribute((
                    "address_prefixes",
                    list(std::iter::once(&p.cidr).chain(p.ipv6.as_ref().map(|v| &v.cidr)).map(|c| string(c.as_str()))),
                ))
                // An offline network gives an interface without a public address no way out:
                // the cloud enforces `internet: false` besides the machines' own rules.
                .add_attribute(("default_outbound_access_enabled", boolean(p.internet)))
                .build(),
        );
    }
}

/// A resource in the environment's resource group and region, tagged.
pub(super) fn placed(kind: &str, name: &str, cloud_name: Expression) -> BlockBuilder {
    resource(kind, name)
        .add_attribute(("name", cloud_name))
        .add_attribute(("resource_group_name", reference("azurerm_resource_group.env.name")))
        .add_attribute(("location", reference("var.region")))
        .add_attribute(("tags", reference("local.tags")))
}

/// A network security group: inbound rules numbered from 100 in the order given, then
/// everything out.
pub(super) struct Nsg {
    block: BlockBuilder,
    priority: u64,
}

impl Nsg {
    /// The group `id` (its name in the cloud: `<env>-<who>`).
    pub(super) fn new(id: &str, who: &str) -> Nsg {
        Nsg {
            block: placed("azurerm_network_security_group", id, name_with(&format!("-{who}"))),
            priority: 100,
        }
    }

    /// An inbound allow rule (`protocol`: `*`, `Tcp` or `Udp`; `port`: a port or `*`).
    #[must_use]
    pub(super) fn allow(mut self, name: &str, protocol: &str, port: &str, from: Expression) -> Nsg {
        self.block = self.block.add_block(rule(name, self.priority, ("Inbound", protocol, port), from));
        self.priority += 10;
        self
    }

    /// The group, everything out allowed (Azure's default, made explicit as on AWS).
    pub(super) fn build(self) -> Block {
        self.block.add_block(rule("outbound", 100, ("Outbound", "*", "*"), string("*"))).build()
    }
}

/// One allow rule: (direction, protocol, port).
fn rule(name: &str, priority: u64, (direction, protocol, port): (&str, &str, &str), from: Expression) -> Block {
    Block::builder("security_rule")
        .add_attribute(("name", string(name)))
        .add_attribute(("priority", number(priority)))
        .add_attribute(("direction", string(direction)))
        .add_attribute(("access", string("Allow")))
        .add_attribute(("protocol", string(protocol)))
        .add_attribute(("source_port_range", string("*")))
        .add_attribute(("destination_port_range", string(port)))
        .add_attribute(("source_address_prefix", from))
        .add_attribute(("destination_address_prefix", string("*")))
        .build()
}

/// A machine's public IP (static, so the address `ip` gives holds across a stop).
pub(super) fn public_ip(id: &str, who: &str) -> Block {
    placed("azurerm_public_ip", id, name_with(&format!("-{who}")))
        .add_attribute(("allocation_method", string("Static")))
        .add_attribute(("sku", string("Standard")))
        .build()
}

/// A machine's interface on one network.
pub(super) struct Nic<'a> {
    /// The machine's resource name (`isoloom_controller` for the controller).
    pub(super) id: &'a str,
    /// Its name in the cloud (`<env>-<who>-<network>`).
    pub(super) who: &'a str,
    /// The network.
    pub(super) net: &'a str,
    /// Its address there.
    pub(super) address: Ipv4Addr,
    /// Its IPv6 address there, on a dual-stack network.
    pub(super) address6: Option<Ipv6Addr>,
    /// Its further addresses there (`lan: [10, 11]`), each an IP configuration of its own.
    pub(super) more: &'a [Ipv4Addr],
    /// The first interface carries the public IP.
    pub(super) primary: bool,
    /// Forwarding on: the machine is on several networks.
    pub(super) forwarding: bool,
}

/// The interface's resource name.
pub(super) fn nic_id(id: &str, net: &str) -> String {
    format!("{id}_{}", ident(net))
}

/// The interface at its address (with the public IP when it's the primary one) and its
/// association with the machine's security group.
pub(super) fn nic(n: &Nic) -> [Block; 2] {
    let nid = nic_id(n.id, n.net);
    let mut config = Block::builder("ip_configuration")
        .add_attribute(("name", string("primary")))
        .add_attribute(("subnet_id", reference(&format!("azurerm_subnet.{}.id", ident(n.net)))))
        .add_attribute(("private_ip_address_allocation", string("Static")))
        .add_attribute(("private_ip_address", string(n.address.to_string())));
    if n.primary {
        config = config.add_attribute(("public_ip_address_id", reference(&format!("azurerm_public_ip.{}.id", n.id))));
    }
    // With several, Azure wants the first named the primary one.
    if !n.more.is_empty() {
        config = config.add_attribute(("primary", boolean(true)));
    }
    let mut b = placed("azurerm_network_interface", &nid, name_with(&format!("-{}-{}", n.who, n.net)));
    if n.forwarding {
        b = b.add_attribute(("ip_forwarding_enabled", boolean(true)));
    }
    b = b.add_block(config.build());
    for (i, more) in n.more.iter().enumerate() {
        b = b.add_block(
            Block::builder("ip_configuration")
                .add_attribute(("name", string(format!("more-{}", i + 1))))
                .add_attribute(("subnet_id", reference(&format!("azurerm_subnet.{}.id", ident(n.net)))))
                .add_attribute(("private_ip_address_allocation", string("Static")))
                .add_attribute(("private_ip_address", string(more.to_string())))
                .build(),
        );
    }
    if let Some(v6) = n.address6 {
        b = b.add_block(
            Block::builder("ip_configuration")
                .add_attribute(("name", string("ipv6")))
                .add_attribute(("subnet_id", reference(&format!("azurerm_subnet.{}.id", ident(n.net)))))
                .add_attribute(("private_ip_address_version", string("IPv6")))
                .add_attribute(("private_ip_address_allocation", string("Static")))
                .add_attribute(("private_ip_address", string(v6.to_string())))
                .build(),
        );
    }
    [
        b.build(),
        resource("azurerm_network_interface_security_group_association", &nid)
            .add_attribute(("network_interface_id", reference(&format!("azurerm_network_interface.{nid}.id"))))
            .add_attribute(("network_security_group_id", reference(&format!("azurerm_network_security_group.{}.id", n.id))))
            .build(),
    ]
}
