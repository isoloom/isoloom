//! AWS's `cloud-docker` module: a VPC with one public subnet, a security group, and an instance
//! of the Docker host's OS from AWS's image table (it terminates on shutdown, so its expiry ends
//! it), every resource tagged as on `cloud-vm` (the shared provider).

use crate::hcl_builder::{Block, BlockBuilder, Expression, Module, boolean, call, list, object, reference, string};
use isoloom_plan::Environment;
use isoloom_render::container::HostSize;
use isoloom_spec::model::Protocol;

use super::{ec2_instance, egress, first_zone, ingress, instance_type, key_pair, root_disk, route_out, vm, zones};
use crate::cloud::docker::{Driver, HOST_OS, SizeVariable};
use crate::cloud::instance;
use crate::docker_host::published;
use crate::module::{ident, providers};

/// Its `cloud-docker` driver.
pub(crate) const DRIVER: Driver = Driver {
    base: super::BASE,
    requires: providers::AWS,
    // Free Tier eligible sizes where they fit (the AWS Free plan refuses other types).
    size: Some(SizeVariable {
        name: "instance_type",
        default: |s| instance_type(s.memory_mb),
    }),
    resources,
};

/// The cloud's resources, after the module's head, `user_data` in the instance; the Docker
/// host's address.
fn resources(env: &Environment, m: &mut Module, user_data: Expression) -> Expression {
    let size = HostSize::of(env);
    m.block(zones());
    m.blocks(vm::ami_lookup(HOST_OS));
    m.blocks(network());
    m.block(security_group(env));
    m.block(host(size, user_data));
    reference("aws_instance.env.public_ip")
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

/// The instance ([`ec2_instance`]: terminated when it shuts down): its expiry in its user data
/// (gzipped: AWS takes 16 KB), run once ([`instance::set_up_once`]).
fn host(size: HostSize, user_data: Expression) -> Block {
    ec2_instance("env")
        .add_attribute(("ami", reference(&format!("data.aws_ami.{}.id", ident(HOST_OS)))))
        .add_attribute(("instance_type", reference("var.instance_type")))
        .add_attribute(("subnet_id", reference("aws_subnet.env.id")))
        .add_attribute(("key_name", reference("aws_key_pair.env.key_name")))
        .add_attribute(("vpc_security_group_ids", list([reference("aws_security_group.env.id")])))
        .add_attribute(("user_data_base64", call("base64gzip", [user_data])))
        .add_block(instance::set_up_once(reference("user_data_base64")).build())
        .add_block(root_disk(size.disk_gb))
        .add_attribute(("depends_on", list([reference("aws_route_table_association.env")])))
        .add_attribute(("tags", named()))
        .build()
}
