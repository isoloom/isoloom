//! AWS: what its `cloud-vm` module ([`vm`]) and its `cloud-docker` module ([`docker`]) share:
//! the region, the provider, the availability zones, an AMI lookup, the key pair, the route to
//! the internet, security group rules and the instance types.

pub(crate) mod docker;
pub(crate) mod vm;

use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, boolean, index, list, number, reference, string};

use crate::module::resource;
use crate::module::variable::Variable;

/// The `region` variable: Paris by default.
pub(crate) fn region() -> Block {
    Variable::new("region", "string").default(string("eu-west-3")).block()
}

/// The provider in the region, every resource tagged with `tags`.
pub(crate) fn provider(tags: Expression) -> Block {
    Block::builder("provider")
        .add_label("aws")
        .add_attribute(("region", reference("var.region")))
        .add_block(Block::builder("default_tags").add_attribute(("tags", tags)).build())
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
    Block::builder("egress")
        .add_attribute(("from_port", number(0)))
        .add_attribute(("to_port", number(0)))
        .add_attribute(("protocol", string("-1")))
        .add_attribute(("cidr_blocks", list([string("0.0.0.0/0")])))
        .build()
}

/// The instance type for an amount of memory: Free Tier eligible up to 8 GB (accounts on the
/// AWS Free plan can't launch other types), then general purpose.
pub(crate) fn instance_type(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "t3.micro",
        1025..=2048 => "t3.small",
        2049..=4096 => "c7i-flex.large",
        4097..=8192 => "m7i-flex.large",
        _ => "m7i-flex.xlarge",
    }
}
