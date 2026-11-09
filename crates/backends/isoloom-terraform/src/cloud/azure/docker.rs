//! Azure's `cloud-docker` module: a resource group holding a virtual network and subnet, a
//! static public address, a security group, and a VM of the Docker host's OS, every resource
//! tagged.

use isoloom_plan::Environment;
use isoloom_render::container::HostSize;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, call, list, number, reference, string};
use isoloom_spec::model::Protocol;

use super::vm::{self, USER};
use crate::cloud::docker::{Driver, HOST_OS, SizeVariable};
use crate::cloud::instance::set_up_once;
use crate::docker_host::{port_strings, published, tcp_ports};
use crate::module::providers;

/// Its `cloud-docker` driver.
pub(crate) const DRIVER: Driver = Driver {
    base: super::BASE,
    requires: providers::AZURERM,
    size: Some(SizeVariable {
        name: "size",
        default: |s| s.pick(["Standard_D2als_v6", "Standard_D2as_v6", "Standard_D4as_v6"]),
    }),
    resources,
};

/// The cloud's resources, after the module's head, `custom_data` in the VM (read by
/// cloud-init); the Docker host's address.
fn resources(env: &Environment, m: &mut Module, custom_data: Expression) -> Expression {
    m.blocks(network());
    m.block(security_group(env));
    m.blocks(interface());
    m.block(host(HostSize::of(env), custom_data));
    reference("azurerm_public_ip.env.ip_address")
}

/// `resource "<kind>" "env"`, named after the environment.
fn resource(kind: &str) -> BlockBuilder {
    Block::builder("resource")
        .add_label(kind)
        .add_label("env")
        .add_attribute(("name", reference("local.name")))
}

/// In the resource group, in the region, with the tags every resource carries.
fn placed(kind: &str) -> BlockBuilder {
    resource(kind)
        .add_attribute(("resource_group_name", reference("azurerm_resource_group.env.name")))
        .add_attribute(("location", reference("var.region")))
        .add_attribute(("tags", reference("local.tags")))
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
    let tcp = tcp_ports(env);
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
        crate::module::resource("azurerm_network_interface_security_group_association", "env")
            .add_attribute(("network_interface_id", reference("azurerm_network_interface.env.id")))
            .add_attribute(("network_security_group_id", reference("azurerm_network_security_group.env.id")))
            .build(),
    ]
}

/// The Docker host's OS from Azure's image table, the cloud's user with the runner's key, its
/// disk as a `cloud-vm` machine's, its expiry in its custom data, run once ([`set_up_once`]).
fn host(size: HostSize, custom_data: Expression) -> Block {
    placed("azurerm_linux_virtual_machine")
        .add_attribute(("size", reference("var.size")))
        .add_attribute(("custom_data", call("base64encode", [custom_data])))
        .add_attribute(("admin_username", string(USER)))
        .add_attribute(("network_interface_ids", list([reference("azurerm_network_interface.env.id")])))
        .add_block(
            Block::builder("admin_ssh_key")
                .add_attribute(("username", string(USER)))
                .add_attribute(("public_key", reference("var.ssh_public_key")))
                .build(),
        )
        .add_block(vm::os_disk(size.disk_gb))
        .add_block(vm::source_image(HOST_OS, false))
        .add_attribute(("depends_on", list([reference("azurerm_network_interface_security_group_association.env")])))
        .add_block(set_up_once(reference("custom_data")).build())
        .build()
}
