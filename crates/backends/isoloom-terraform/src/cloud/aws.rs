//! AWS: what its `cloud-vm` module ([`vm`]) and its `cloud-docker` module ([`docker`]) share:
//! the region, the provider, the availability zones, an AMI lookup, the key pair, the route to
//! the internet, security group rules and the instance types.

pub(crate) mod docker;
pub(crate) mod vm;

use crate::hcl_builder::{Block, BlockBuilder, Expression, boolean, index, list, number, reference, string};

use super::{Base, instance, operator};
use crate::cloud::vm::rules::{Need, Size, Sizes};
use crate::module::{Usage, resource};

/// What AWS's modules share: who may reach the machines and their key, the region, the provider
/// tagging every resource ([`instance::tags`]), and its images' users.
pub(crate) const BASE: Base = Base {
    cloud: crate::cloud::Cloud::Aws,
    usage: |folder| Usage::with_runner_key(folder, operator::ALLOWED),
    variables: |region| vec![crate::cloud::region_variable(region)],
    provider: |env, m| {
        m.block(provider(instance::tags(env)));
        Vec::new()
    },
    user: vm::image_user,
};

/// The provider in the region, every resource tagged with `tags`.
pub(crate) fn provider(tags: Expression) -> Block {
    Block::builder("provider")
        .add_label("aws")
        .add_attribute(("region", reference("var.region")))
        .add_block(Block::builder("default_tags").add_attribute(("tags", tags)).build())
        .build()
}

/// `resource "aws_instance" "<id>"`, the same on both targets: it terminates when it shuts down
/// (a machine's own expiry shutdown ends it, nothing is left stopped and billed for its disk),
/// and its metadata service takes session tokens only (`IMDSv2`: a request forwarded by an environment's
/// service can't read it).
pub(crate) fn ec2_instance(id: &str) -> BlockBuilder {
    resource("aws_instance", id)
        .add_attribute(("instance_initiated_shutdown_behavior", string("terminate")))
        .add_block(Block::builder("metadata_options").add_attribute(("http_tokens", string("required"))).build())
}

/// An instance's root disk of `gb` GB, the same on both targets: gp3, encrypted.
pub(crate) fn root_disk(gb: u32) -> Block {
    Block::builder("root_block_device")
        .add_attribute(("volume_size", number(u64::from(gb))))
        .add_attribute(("volume_type", string("gp3")))
        .add_attribute(("encrypted", boolean(true)))
        .build()
}

/// The region's availability zones.
pub(crate) fn zones() -> Block {
    Block::builder("data")
        .add_label("aws_availability_zones")
        .add_label("available")
        .add_attribute(("state", string("available")))
        .build()
}

/// The first of the region's availability zones, where the subnets go.
pub(crate) fn first_zone() -> Expression {
    index(reference("data.aws_availability_zones.available.names"), 0)
}

/// An AMI lookup, `data.aws_ami.<name>`: the newest image of `owner` named like `filter`.
pub(crate) fn ami(name: &str, owner: &str, filter: &str) -> BlockBuilder {
    Block::builder("data")
        .add_label("aws_ami")
        .add_label(name)
        .add_attribute(("most_recent", boolean(true)))
        .add_attribute(("owners", list([string(owner)])))
        .add_block(ami_filter("name", filter))
}

/// One `filter` of an AMI lookup.
pub(crate) fn ami_filter(name: &str, value: &str) -> Block {
    Block::builder("filter")
        .add_attribute(("name", string(name)))
        .add_attribute(("values", list([string(value)])))
        .build()
}

/// The operator's key, `aws_key_pair.env`.
pub(crate) fn key_pair() -> Block {
    resource("aws_key_pair", "env")
        .add_attribute(("key_name", reference("local.name")))
        .add_attribute(("public_key", reference("var.ssh_public_key")))
        .build()
}

/// A route table's way out: the internet through the internet gateway.
pub(crate) fn route_out() -> Block {
    Block::builder("route")
        .add_attribute(("cidr_block", string("0.0.0.0/0")))
        .add_attribute(("gateway_id", reference("aws_internet_gateway.env.id")))
        .build()
}

/// A security group's `ingress` rule from `cidrs`, on the ports `from` to `to`.
pub(crate) fn ingress(description: &str, (from, to): (u16, u16), protocol: &str, cidrs: Expression) -> Block {
    Block::builder("ingress")
        .add_attribute(("description", string(description)))
        .add_attribute(("from_port", number(u64::from(from))))
        .add_attribute(("to_port", number(u64::from(to))))
        .add_attribute(("protocol", string(protocol)))
        .add_attribute(("cidr_blocks", cidrs))
        .build()
}

/// A security group's way out: everything.
pub(crate) fn egress() -> Block {
    egress_to(list([string("0.0.0.0/0")]))
}

/// A security group's `egress` rule to `cidrs`, every protocol (a sealed machine's: the environment's
/// own blocks).
pub(crate) fn egress_to(cidrs: Expression) -> Block {
    Block::builder("egress")
        .add_attribute(("from_port", number(0)))
        .add_attribute(("to_port", number(0)))
        .add_attribute(("protocol", string("-1")))
        .add_attribute(("cidr_blocks", cidrs))
        .build()
}

/// The route out of an `internet: false` network's subnet: the internet until the run is sealed,
/// then `allowed_cidr` alone (the runner's SSH and the published ports' replies).
pub(crate) fn sealed_route_out() -> Block {
    let cidr = crate::cloud::vm::seal::when_sealed(reference("var.allowed_cidr"), string("0.0.0.0/0"));
    Block::builder("route")
        .add_attribute(("cidr_block", cidr))
        .add_attribute(("gateway_id", reference("aws_internet_gateway.env.id")))
        .build()
}

/// The instance types Isoloom picks, smallest first: Free Tier eligible up to 8 GB (accounts on
/// the AWS Free plan can't launch other types), then general purpose. Each with its vCPUs, its
/// memory, and AWS's "maximum network interfaces" and "IPv4 addresses per interface"
/// (<https://docs.aws.amazon.com/ec2/latest/instancetypes/gp.html#gp_network>).
pub(crate) const INSTANCE_TYPES: Sizes = Sizes::ordered(&[
    Size::new("t3.micro", 2, 1024, 2).with_addresses(2),
    Size::new("t3.small", 2, 2048, 3).with_addresses(4),
    Size::new("c7i-flex.large", 2, 4096, 3).with_addresses(10),
    Size::new("m7i-flex.large", 2, 8192, 3).with_addresses(10),
    Size::new("m7i-flex.xlarge", 4, 16384, 4).with_addresses(15),
]);

/// The `cloud-docker` host's instance type for `memory_mb`: the smallest of [`INSTANCE_TYPES`]
/// with that memory, the host sized by its memory alone (`HostSizes::ByMemory`).
pub(crate) fn host_instance_type(memory_mb: u32) -> &'static str {
    INSTANCE_TYPES.fit(&Need::new(1, memory_mb, 0))
}
