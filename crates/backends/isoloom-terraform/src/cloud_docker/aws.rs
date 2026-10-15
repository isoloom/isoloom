//! AWS: a VPC with one public subnet, a security group, and a Debian instance whose user data
//! schedules the auto-stop (it terminates on shutdown).

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Expression, Module, boolean, index, list, null, number, object, reference, string};
use isoloom_spec::model::Protocol;

use super::host::{self, Reach, Sizing, published};
use crate::cloud_vm::aws::instance_type;
use crate::vm::cloud;
use crate::vm::module::{Variable, terraform};
use crate::vm::versions;

/// The module, from the plan.
pub fn module(env: &Environment) -> Module {
    let mut m = Module::new(host::preamble(super::DIR, "aws", "-var allowed_cidr=<your IP>/32 "));
    m.block(terraform(&[versions::AWS, versions::LOCAL]));
    m.block(Variable::new("region", "string").default(string("eu-west-3")).block());
    // Free Tier eligible sizes where they fit (the AWS Free plan refuses other types).
    let size = Sizing::of(env);
    m.block(Variable::new("instance_type", "string").default(string(instance_type(size.memory_mb))).block());
    m.blocks(host::variables(env, true));
    m.block(provider(env));
    m.blocks(cloud::id_and_locals(env, Vec::new()));
    m.blocks(lookups());
    m.blocks(network());
    m.block(security_group(env));
    m.block(instance(size));
    let reach = Reach {
        host: reference("aws_instance.env.public_ip"),
        user: "admin",
        replace_on: reference("aws_instance.env.id"),
        auto_stop: false,
    };
    host::environment(env, &reach, &mut m);
    m
}

/// Every resource tagged as the environment's.
fn provider(env: &Environment) -> Block {
    let tags = object([
        ("isoloom:environment", string(env.name.as_str())),
        ("managed-by", string("isoloom")),
        ("isoloom-expires-at", reference("var.expires_at")),
    ]);
    Block::builder("provider")
        .add_label("aws")
        .add_attribute(("region", reference("var.region")))
        .add_block(Block::builder("default_tags").add_attribute(("tags", tags)).build())
        .build()
}

/// The zones, and Debian's latest image.
fn lookups() -> Vec<Block> {
    vec![
        Block::builder("data")
            .add_label("aws_availability_zones")
            .add_label("available")
            .add_attribute(("state", string("available")))
            .build(),
        Block::builder("data")
            .add_label("aws_ami")
            .add_label("debian")
            .add_attribute(("most_recent", boolean(true)))
            .add_attribute(("owners", list([string("136693071363")])))
            .add_block(
                Block::builder("filter")
                    .add_attribute(("name", string("name")))
                    .add_attribute(("values", list([string("debian-12-amd64-*")])))
                    .build(),
            )
            .build(),
    ]
}

fn resource(kind: &str) -> isoloom_render::hcl_builder::BlockBuilder {
    Block::builder("resource").add_label(kind).add_label("env")
}

fn named() -> Expression {
    object([("Name", reference("local.name"))])
}

/// The VPC, its internet gateway, the subnet and its route out, and the key pair.
fn network() -> Vec<Block> {
    vec![
        resource("aws_vpc")
            .add_attribute(("cidr_block", string("10.42.0.0/16")))
            .add_attribute(("tags", named()))
            .build(),
        resource("aws_internet_gateway")
            .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
            .add_attribute(("tags", named()))
            .build(),
        resource("aws_subnet")
            .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
            .add_attribute(("cidr_block", string("10.42.1.0/24")))
            .add_attribute(("availability_zone", index(reference("data.aws_availability_zones.available.names"), 0)))
            .add_attribute(("map_public_ip_on_launch", boolean(true)))
            .add_attribute(("tags", named()))
            .build(),
        resource("aws_route_table")
            .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
            .add_block(
                Block::builder("route")
                    .add_attribute(("cidr_block", string("0.0.0.0/0")))
                    .add_attribute(("gateway_id", reference("aws_internet_gateway.env.id")))
                    .build(),
            )
            .add_attribute(("tags", named()))
            .build(),
        resource("aws_route_table_association")
            .add_attribute(("subnet_id", reference("aws_subnet.env.id")))
            .add_attribute(("route_table_id", reference("aws_route_table.env.id")))
            .build(),
        resource("aws_key_pair")
            .add_attribute(("key_name", reference("local.name")))
            .add_attribute(("public_key", reference("var.ssh_public_key")))
            .build(),
    ]
}

/// One ingress rule from `allowed_cidr`.
fn ingress(description: &str, port: u16, protocol: &str) -> Block {
    Block::builder("ingress")
        .add_attribute(("description", string(description)))
        .add_attribute(("from_port", number(port.into())))
        .add_attribute(("to_port", number(port.into())))
        .add_attribute(("protocol", string(protocol)))
        .add_attribute(("cidr_blocks", list([reference("var.allowed_cidr")])))
        .build()
}

/// SSH and the published ports from `allowed_cidr`; everything out.
fn security_group(env: &Environment) -> Block {
    let mut b = resource("aws_security_group")
        .add_attribute(("name", reference("local.name")))
        .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
        .add_block(ingress("SSH", 22, "tcp"));
    for p in published(env, Protocol::Tcp) {
        b = b.add_block(ingress(&format!("published {p}"), p, "tcp"));
    }
    for p in published(env, Protocol::Udp) {
        b = b.add_block(ingress(&format!("published {p}/udp"), p, "udp"));
    }
    b.add_block(
        Block::builder("egress")
            .add_attribute(("from_port", number(0)))
            .add_attribute(("to_port", number(0)))
            .add_attribute(("protocol", string("-1")))
            .add_attribute(("cidr_blocks", list([string("0.0.0.0/0")])))
            .build(),
    )
    .build()
}

/// The instance: the auto-stop in its user data, terminated when it shuts down.
fn instance(size: Sizing) -> Block {
    resource("aws_instance")
        .add_attribute(("ami", reference("data.aws_ami.debian.id")))
        .add_attribute(("instance_type", reference("var.instance_type")))
        .add_attribute(("subnet_id", reference("aws_subnet.env.id")))
        .add_attribute(("key_name", reference("aws_key_pair.env.key_name")))
        .add_attribute(("vpc_security_group_ids", list([reference("aws_security_group.env.id")])))
        .add_attribute(("instance_initiated_shutdown_behavior", string("terminate")))
        .add_attribute(("user_data", cloud::auto_stop(cloud::shutdown_script(), null())))
        .add_block(
            Block::builder("root_block_device")
                .add_attribute(("volume_size", number(size.disk_gb.into())))
                .add_attribute(("volume_type", string("gp3")))
                .add_attribute(("encrypted", boolean(true)))
                .build(),
        )
        .add_block(Block::builder("metadata_options").add_attribute(("http_tokens", string("required"))).build())
        .add_attribute(("depends_on", list([reference("aws_route_table_association.env")])))
        .add_attribute(("tags", named()))
        .build()
}
