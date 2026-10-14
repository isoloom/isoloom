//! Azure: a resource group holding a virtual network and subnet, a static public address, a
//! security group, and a Debian VM.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, list, null, number, reference, string};
use isoloom_spec::model::Protocol;

use super::host::{self, Reach, Sizing, port_strings, published};
use crate::vm::module::{Variable, terraform};

/// The module, from the plan.
pub fn module(env: &Environment) -> Module {
    let mut m = Module::new(host::preamble(super::DIR, "azure", "-var allowed_cidr=<your IP>/32 "));
    m.block(terraform(&[("azurerm", "hashicorp/azurerm", "~> 5.0")]));
    let size = Sizing::of(env).pick(["Standard_D2als_v6", "Standard_D2as_v6", "Standard_D4as_v6"]);
    m.blocks([
        Variable::new("subscription_id", "string")
            .default(null())
            .describe("Default: ARM_SUBSCRIPTION_ID from the environment")
            .block(),
        Variable::new("region", "string").default(string("swedencentral")).block(),
        Variable::new("size", "string").default(string(size)).block(),
    ]);
    m.blocks(host::variables(env, true));
    m.block(
        Block::builder("provider")
            .add_label("azurerm")
            .add_block(Block::builder("features").build())
            .add_attribute(("subscription_id", reference("var.subscription_id")))
            .build(),
    );
    m.block(host::id());
    m.block(host::locals(env, vec![("tags", host::tags(env))]));
    m.blocks(network());
    m.block(security_group(env));
    m.blocks(interface());
    m.block(vm());
    let reach = Reach {
        host: reference("azurerm_public_ip.env.ip_address"),
        user: "isoloom",
        replace_on: reference("azurerm_linux_virtual_machine.env.id"),
        auto_stop: true,
    };
    host::environment(env, &reach, &mut m);
    m
}

/// `resource "<kind>" "env"`, named after the environment.
fn resource(kind: &str) -> BlockBuilder {
    Block::builder("resource")
        .add_label(kind)
        .add_label("env")
        .add_attribute(("name", reference("local.name")))
}

/// In the resource group, in the region.
fn placed(kind: &str) -> BlockBuilder {
    resource(kind)
        .add_attribute(("resource_group_name", reference("azurerm_resource_group.env.name")))
        .add_attribute(("location", reference("var.region")))
}

/// The resource group, the network, its subnet and the public address.
fn network() -> Vec<Block> {
    vec![
        resource("azurerm_resource_group")
            .add_attribute(("location", reference("var.region")))
            .add_attribute(("tags", reference("local.tags")))
            .build(),
        placed("azurerm_virtual_network")
            .add_attribute(("address_space", list([string("10.42.0.0/16")])))
            .build(),
        resource("azurerm_subnet")
            .add_attribute(("resource_group_name", reference("azurerm_resource_group.env.name")))
            .add_attribute(("virtual_network_name", reference("azurerm_virtual_network.env.name")))
            .add_attribute(("address_prefixes", list([string("10.42.1.0/24")])))
            .build(),
        placed("azurerm_public_ip")
            .add_attribute(("allocation_method", string("Static")))
            .add_attribute(("sku", string("Standard")))
            .build(),
    ]
}

/// An inbound rule from `allowed_cidr`.
fn rule(name: &str, priority: u64, protocol: &str, ports: Expression) -> Block {
    Block::builder("security_rule")
        .add_attribute(("name", string(name)))
        .add_attribute(("priority", number(priority)))
        .add_attribute(("direction", string("Inbound")))
        .add_attribute(("access", string("Allow")))
        .add_attribute(("protocol", string(protocol)))
        .add_attribute(("source_port_range", string("*")))
        .add_attribute(("destination_port_ranges", ports))
        .add_attribute(("source_address_prefix", reference("var.allowed_cidr")))
        .add_attribute(("destination_address_prefix", string("*")))
        .build()
}

/// SSH and the published TCP ports, then the published UDP ports.
fn security_group(env: &Environment) -> Block {
    let mut tcp = vec![22];
    tcp.extend(published(env, Protocol::Tcp));
    let mut b = placed("azurerm_network_security_group").add_block(rule("ssh-and-published", 100, "Tcp", port_strings(&tcp)));
    let udp = published(env, Protocol::Udp);
    if !udp.is_empty() {
        b = b.add_block(rule("published-udp", 110, "Udp", port_strings(&udp)));
    }
    b.build()
}

/// The VM's interface, with the public address and the security group.
fn interface() -> Vec<Block> {
    vec![
        placed("azurerm_network_interface")
            .add_block(
                Block::builder("ip_configuration")
                    .add_attribute(("name", string("env")))
                    .add_attribute(("subnet_id", reference("azurerm_subnet.env.id")))
                    .add_attribute(("private_ip_address_allocation", string("Dynamic")))
                    .add_attribute(("public_ip_address_id", reference("azurerm_public_ip.env.id")))
                    .build(),
            )
            .build(),
        Block::builder("resource")
            .add_label("azurerm_network_interface_security_group_association")
            .add_label("env")
            .add_attribute(("network_interface_id", reference("azurerm_network_interface.env.id")))
            .add_attribute(("network_security_group_id", reference("azurerm_network_security_group.env.id")))
            .build(),
    ]
}

/// Debian 12, the `isoloom` user with the operator's key.
fn vm() -> Block {
    placed("azurerm_linux_virtual_machine")
        .add_attribute(("size", reference("var.size")))
        .add_attribute(("admin_username", string("isoloom")))
        .add_attribute(("network_interface_ids", list([reference("azurerm_network_interface.env.id")])))
        .add_block(
            Block::builder("admin_ssh_key")
                .add_attribute(("username", string("isoloom")))
                .add_attribute(("public_key", reference("var.ssh_public_key")))
                .build(),
        )
        .add_block(
            Block::builder("os_disk")
                .add_attribute(("caching", string("ReadWrite")))
                .add_attribute(("storage_account_type", string("StandardSSD_LRS")))
                .build(),
        )
        .add_block(
            Block::builder("source_image_reference")
                .add_attribute(("publisher", string("Debian")))
                .add_attribute(("offer", string("debian-12")))
                .add_attribute(("sku", string("12-gen2")))
                .add_attribute(("version", string("latest")))
                .build(),
        )
        .add_attribute(("tags", reference("local.tags")))
        .add_attribute(("depends_on", list([reference("azurerm_network_interface_security_group_association.env")])))
        .build()
}
