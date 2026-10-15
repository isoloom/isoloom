//! AWS's `cloud-docker` module: a VPC with one public subnet, a security group, and a Debian
//! instance whose user data schedules the auto-stop (it terminates on shutdown).

use isoloom_plan::Environment;
use isoloom_render::container::HostSize;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, boolean, list, null, number, object, reference, string};
use isoloom_spec::model::Protocol;

use super::{ami, egress, first_zone, ingress, instance_type, key_pair, provider, region, route_out, zones};
use crate::cloud::auto_stop;
use crate::cloud::docker::{Driver, SizeVariable};
use crate::docker_host::{Access, AutoStop, published};
use crate::module::providers;

/// Its `cloud-docker` driver.
pub(crate) const DRIVER: Driver = Driver {
    requires: providers::AWS,
    variables: || vec![region()],
    // Free Tier eligible sizes where they fit (the AWS Free plan refuses other types).
    size: Some(SizeVariable {
        name: "instance_type",
        default: |s| instance_type(s.memory_mb),
    }),
    provider: |env, m| {
        m.block(provider(tags(env)));
        Vec::new()
    },
    resources,
};

/// The cloud's resources, after the module's head; the host they make, as its set-up reaches
/// it.
fn resources(env: &Environment, m: &mut Module) -> Access {
    let size = HostSize::of(env);
    m.block(zones());
    m.block(ami("debian", "136693071363", "debian-12-amd64-*").build());
    m.blocks(network());
    m.block(security_group(env));
    m.block(vm(size));
    Access {
        host: reference("aws_instance.env.public_ip"),
        user: "admin",
        replace_on: reference("aws_instance.env.id"),
        auto_stop: AutoStop::UserData,
    }
}

/// The tags on every resource: the environment's, who manages it, its expiry.
fn tags(env: &Environment) -> Expression {
    object([
        ("isoloom:environment", string(env.name.as_str())),
        ("managed-by", string("isoloom")),
        ("isoloom-expires-at", reference("var.expires_at")),
    ])
}

/// `resource "<kind>" "env"`.
fn resource(kind: &str) -> BlockBuilder {
    crate::module::resource(kind, "env")
}

/// Its name tag: the environment's instance.
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
            .add_attribute(("availability_zone", first_zone()))
            .add_attribute(("map_public_ip_on_launch", boolean(true)))
            .add_attribute(("tags", named()))
            .build(),
        resource("aws_route_table")
            .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
            .add_block(route_out())
            .add_attribute(("tags", named()))
            .build(),
        resource("aws_route_table_association")
            .add_attribute(("subnet_id", reference("aws_subnet.env.id")))
            .add_attribute(("route_table_id", reference("aws_route_table.env.id")))
            .build(),
        key_pair(),
    ]
}

/// SSH and the published ports from `allowed_cidr`; everything out.
fn security_group(env: &Environment) -> Block {
    let allowed = || list([reference("var.allowed_cidr")]);
    let mut b = resource("aws_security_group")
        .add_attribute(("name", reference("local.name")))
        .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
        .add_block(ingress("SSH", (22, 22), "tcp", allowed()));
    for p in published(env, Protocol::Tcp) {
        b = b.add_block(ingress(&format!("published {p}"), (p, p), "tcp", allowed()));
    }
    for p in published(env, Protocol::Udp) {
        b = b.add_block(ingress(&format!("published {p}/udp"), (p, p), "udp", allowed()));
    }
    b.add_block(egress()).build()
}

/// The instance: the auto-stop in its user data, terminated when it shuts down.
fn vm(size: HostSize) -> Block {
    resource("aws_instance")
        .add_attribute(("ami", reference("data.aws_ami.debian.id")))
        .add_attribute(("instance_type", reference("var.instance_type")))
        .add_attribute(("subnet_id", reference("aws_subnet.env.id")))
        .add_attribute(("key_name", reference("aws_key_pair.env.key_name")))
        .add_attribute(("vpc_security_group_ids", list([reference("aws_security_group.env.id")])))
        .add_attribute(("instance_initiated_shutdown_behavior", string("terminate")))
        .add_attribute(("user_data", auto_stop::if_set(auto_stop::script(), null())))
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
