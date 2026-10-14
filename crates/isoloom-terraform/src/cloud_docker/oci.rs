//! Oracle Cloud: a VCN with an internet gateway and its route, a security list, a subnet, and
//! an Ubuntu instance (Oracle Cloud has no Debian image of its own).

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Module, attr, boolean, index, list, number, object, reference, string};
use isoloom_spec::model::Protocol;

use super::host::{self, Reach, Sizing, published};
use crate::vm::cloud;
use crate::vm::module::{Variable, terraform};

/// The instance's shape: AMD, sized by its configuration.
const SHAPE: &str = "VM.Standard.E4.Flex";

/// The module, from the plan.
pub fn module(env: &Environment) -> Module {
    let mut m = Module::new(host::preamble(super::DIR, "oci", "-var allowed_cidr=<your IP>/32 "));
    m.block(terraform(&[("oci", "oracle/oci", "~> 9.0")]));
    m.blocks([
        Variable::new("compartment_id", "string").block(),
        Variable::new("region", "string").default(string("eu-paris-1")).block(),
    ]);
    m.blocks(host::variables(env, true));
    m.block(
        Block::builder("provider")
            .add_label("oci")
            .add_attribute(("region", reference("var.region")))
            .build(),
    );
    m.blocks(cloud::id_and_locals(env, Vec::new()));
    m.blocks(lookups());
    m.blocks(network(env));
    m.block(instance(env));
    let reach = Reach {
        host: reference("oci_core_instance.env.public_ip"),
        user: "ubuntu",
        replace_on: reference("oci_core_instance.env.id"),
        auto_stop: true,
    };
    host::environment(env, &reach, &mut m);
    m
}

/// The availability domains, and the latest Ubuntu 24.04 for the shape.
fn lookups() -> Vec<Block> {
    let ads = Block::builder("data")
        .add_label("oci_identity_availability_domains")
        .add_label("ads")
        .add_attribute(("compartment_id", reference("var.compartment_id")))
        .build();
    let ubuntu = Block::builder("data")
        .add_label("oci_core_images")
        .add_label("ubuntu")
        .add_attribute(("compartment_id", reference("var.compartment_id")))
        .add_attribute(("operating_system", string("Canonical Ubuntu")))
        .add_attribute(("operating_system_version", string("24.04")))
        .add_attribute(("shape", string(SHAPE)))
        .add_attribute(("sort_by", string("TIMECREATED")))
        .add_attribute(("sort_order", string("DESC")))
        .build();
    vec![ads, ubuntu]
}

/// `resource "<kind>" "env"` in the compartment.
fn resource(kind: &str) -> BlockBuilder {
    Block::builder("resource")
        .add_label(kind)
        .add_label("env")
        .add_attribute(("compartment_id", reference("var.compartment_id")))
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
        in_vcn("oci_core_route_table")
            .add_block(
                Block::builder("route_rules")
                    .add_attribute(("destination", string("0.0.0.0/0")))
                    .add_attribute(("network_entity_id", reference("oci_core_internet_gateway.env.id")))
                    .build(),
            )
            .build(),
        security_list(env),
        in_vcn("oci_core_subnet")
            .add_attribute(("cidr_block", string("10.42.1.0/24")))
            .add_attribute(("display_name", reference("local.name")))
            .add_attribute(("route_table_id", reference("oci_core_route_table.env.id")))
            .add_attribute(("security_list_ids", list([reference("oci_core_security_list.env.id")])))
            .build(),
    ]
}

fn instance(env: &Environment) -> Block {
    let (ocpus, memory) = Sizing::of(env).pick([(1, 4), (2, 8), (4, 16)]);
    let domain = attr(index(reference("data.oci_identity_availability_domains.ads.availability_domains"), 0), "name");
    resource("oci_core_instance")
        .add_attribute(("availability_domain", domain))
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
