//! Oracle Cloud's `cloud-docker` module: a VCN with an internet gateway and its route, a
//! security list, a subnet, and an Ubuntu instance (Oracle Cloud has no Debian image of its
//! own).

use isoloom_plan::Environment;
use isoloom_render::container::HostSize;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Module, attr, boolean, index, list, number, object, reference, string};
use isoloom_spec::model::Protocol;

use super::{SHAPE, availability_domain, availability_domains, in_compartment, provider, route_out, ubuntu_images};
use crate::cloud::docker::Driver;
use crate::docker_host::{Access, AutoStop, published};
use crate::module::providers;
use crate::module::variable::Variable;

/// Its `cloud-docker` driver: no size variable, its shape's OCPUs and memory follow the sizing.
pub(crate) const DRIVER: Driver = Driver {
    requires: providers::OCI,
    variables: || {
        vec![
            Variable::new("compartment_id", "string").block(),
            Variable::new("region", "string").default(string("eu-paris-1")).block(),
        ]
    },
    size: None,
    provider: |_, m| {
        m.block(provider());
        Vec::new()
    },
    resources,
};

/// The cloud's resources, after the module's head; the host they make, as its set-up reaches
/// it.
fn resources(env: &Environment, m: &mut Module) -> Access {
    m.block(availability_domains());
    m.block(ubuntu_images("ubuntu", "24.04"));
    m.blocks(network(env));
    m.block(vm(env));
    Access {
        host: reference("oci_core_instance.env.public_ip"),
        user: "ubuntu",
        replace_on: reference("oci_core_instance.env.id"),
        auto_stop: AutoStop::SetUp,
    }
}

/// `resource "<kind>" "env"` in the compartment.
fn resource(kind: &str) -> BlockBuilder {
    in_compartment(crate::module::resource(kind, "env"))
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

fn vm(env: &Environment) -> Block {
    let (ocpus, memory) = HostSize::of(env).pick([(1, 4), (2, 8), (4, 16)]);
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
                .add_attribute(("source_id", attr(index(reference("data.oci_core_images.ubuntu.images"), 0), "id")))
                .build(),
        )
        .add_block(
            Block::builder("create_vnic_details")
                .add_attribute(("subnet_id", reference("oci_core_subnet.env.id")))
                .add_attribute(("assign_public_ip", boolean(true)))
                .build(),
        )
        .add_attribute(("metadata", object([("ssh_authorized_keys", reference("var.ssh_public_key"))])))
        .build()
}
