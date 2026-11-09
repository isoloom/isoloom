//! Oracle Cloud's `cloud-docker` module: a VCN with an internet gateway and its route, a
//! security list, a subnet, and an instance of the Docker host's OS from Oracle Cloud's image
//! table: Ubuntu (Oracle Cloud has no Debian image of its own).

use crate::hcl_builder::{Block, BlockBuilder, Expression, Module, attr, boolean, call, entry, index, list, number, object, reference, string};
use isoloom_plan::Environment;
use isoloom_render::container::HostSize;
use isoloom_spec::model::Protocol;

use super::{SHAPE, availability_domain, availability_domains, owned, route_out, ubuntu_images, vm};
use crate::cloud::docker::{Driver, HOST_OS};
use crate::cloud::instance;
use crate::docker_host::published;
use crate::module::providers;

/// Its `cloud-docker` driver: no size variable, its shape's OCPUs and memory follow the sizing.
pub(crate) const DRIVER: Driver = Driver {
    base: super::BASE,
    requires: providers::OCI,
    size: None,
    resources,
};

/// The cloud's resources, after the module's head, `user_data` in the instance's metadata (read
/// by cloud-init); the Docker host's address.
fn resources(env: &Environment, m: &mut Module, user_data: Expression) -> Expression {
    m.block(availability_domains());
    let version = vm::ubuntu(HOST_OS);
    m.block(ubuntu_images(&vm::image_source(version), version));
    m.blocks(network(env));
    m.block(host(env, user_data));
    reference("oci_core_instance.env.public_ip")
}

/// `resource "<kind>" "env"` in the compartment, with its tags.
fn resource(kind: &str) -> BlockBuilder {
    owned(crate::module::resource(kind, "env"))
}

/// In the VCN.
fn in_vcn(kind: &str) -> BlockBuilder {
    resource(kind).add_attribute(("vcn_id", reference("oci_core_vcn.env.id")))
}

/// A rule letting `allowed_cidr` in on one port (`6`: TCP, `17`: UDP).
fn ingress(protocol: &str, options: &str, port: u16) -> Block {
    Block::builder("ingress_security_rules")
        .add_attribute(("source", reference("var.allowed_cidr")))
        .add_attribute(("protocol", string(protocol)))
        .add_block(
            Block::builder(options)
                .add_attribute(("min", number(port.into())))
                .add_attribute(("max", number(port.into())))
                .build(),
        )
        .build()
}

/// SSH and the published ports in from `allowed_cidr`, everything out.
fn security_list(env: &Environment) -> Block {
    let mut b = in_vcn("oci_core_security_list")
        .add_attribute(("display_name", reference("local.name")))
        .add_block(
            Block::builder("egress_security_rules")
                .add_attribute(("destination", string("0.0.0.0/0")))
                .add_attribute(("protocol", string("all")))
                .build(),
        )
        .add_block(ingress("6", "tcp_options", 22));
    for p in published(env, Protocol::Tcp) {
        b = b.add_block(ingress("6", "tcp_options", p));
    }
    for p in published(env, Protocol::Udp) {
        b = b.add_block(ingress("17", "udp_options", p));
    }
    b.build()
}

/// The VCN, its internet gateway and route, the security list and the subnet.
fn network(env: &Environment) -> Vec<Block> {
    vec![
        resource("oci_core_vcn")
            .add_attribute(("display_name", reference("local.name")))
            .add_attribute(("cidr_blocks", list([string("10.42.0.0/16")])))
            .build(),
        in_vcn("oci_core_internet_gateway")
            .add_attribute(("display_name", reference("local.name")))
            .build(),
        in_vcn("oci_core_route_table").add_block(route_out()).build(),
        security_list(env),
        in_vcn("oci_core_subnet")
            .add_attribute(("cidr_block", string("10.42.1.0/24")))
            .add_attribute(("display_name", reference("local.name")))
            .add_attribute(("route_table_id", reference("oci_core_route_table.env.id")))
            .add_attribute(("security_list_ids", list([reference("oci_core_security_list.env.id")])))
            .build(),
    ]
}

/// The instance: Ubuntu on the flexible shape, sized for the containers (its boot volume too, 50
/// GB at least, as Oracle Cloud makes them), its expiry in its user data, run once
/// ([`instance::set_up_once`]).
fn host(env: &Environment, user_data: Expression) -> Block {
    let size = HostSize::of(env);
    let (ocpus, memory) = size.pick([(1, 4), (2, 8), (4, 16)]);
    resource("oci_core_instance")
        .add_attribute(("availability_domain", availability_domain()))
        .add_attribute(("display_name", reference("local.name")))
        .add_attribute(("shape", string(SHAPE)))
        .add_block(
            Block::builder("shape_config")
                .add_attribute(("ocpus", number(ocpus)))
                .add_attribute(("memory_in_gbs", number(memory)))
                .build(),
        )
        .add_block(
            Block::builder("source_details")
                .add_attribute(("source_type", string("image")))
                .add_attribute((
                    "source_id",
                    attr(
                        index(reference(&format!("data.oci_core_images.{}.images", vm::image_source(vm::ubuntu(HOST_OS)))), 0),
                        "id",
                    ),
                ))
                .add_attribute(("boot_volume_size_in_gbs", number(u64::from(size.disk_gb.max(50)))))
                .build(),
        )
        .add_block(
            Block::builder("create_vnic_details")
                .add_attribute(("subnet_id", reference("oci_core_subnet.env.id")))
                .add_attribute(("assign_public_ip", boolean(true)))
                .build(),
        )
        .add_attribute((
            "metadata",
            object([
                ("ssh_authorized_keys", reference("var.ssh_public_key")),
                ("user_data", call("base64encode", [user_data])),
            ]),
        ))
        .add_block(instance::set_up_once(entry(reference("metadata"), "user_data")).build())
        .build()
}
