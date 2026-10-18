//! DigitalOcean: what its `cloud-vm` module ([`vm`]) and its `cloud-docker` module ([`docker`])
//! share: the region, the VPC, the operator's key and the firewall's rules.

pub(crate) mod docker;
pub(crate) mod vm;

use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, reference, string};

use crate::module::resource;
use crate::module::variable::Variable;

/// The `region` variable: Frankfurt by default.
pub(crate) fn region() -> Variable {
    Variable::new("region", "string").default(string("fra1"))
}

/// The VPC, `digitalocean_vpc.env`, with the range `ip_range`.
pub(crate) fn vpc(ip_range: &str) -> Block {
    resource("digitalocean_vpc", "env")
        .add_attribute(("name", reference("local.name")))
        .add_attribute(("region", reference("var.region")))
        .add_attribute(("ip_range", string(ip_range)))
        .build()
}

/// The operator's key, `digitalocean_ssh_key.env`.
pub(crate) fn ssh_key() -> Block {
    resource("digitalocean_ssh_key", "env")
        .add_attribute(("name", reference("local.name")))
        .add_attribute(("public_key", reference("var.ssh_public_key")))
        .build()
}

/// Which way a firewall rule lets traffic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Way {
    /// In, from its addresses.
    In,
    /// Out, to its addresses.
    Out,
}

/// A firewall rule over `protocol` on `ports` (`None`: every port, as ICMP has none), in from or
/// out to `addresses`.
pub(crate) fn rule(way: Way, protocol: &str, ports: Option<&str>, addresses: Expression) -> Block {
    let (block, key) = match way {
        Way::In => ("inbound_rule", "source_addresses"),
        Way::Out => ("outbound_rule", "destination_addresses"),
    };
    let mut b: BlockBuilder = Block::builder(block).add_attribute(("protocol", string(protocol)));
    if let Some(p) = ports {
        b = b.add_attribute(("port_range", string(p)));
    }
    b.add_attribute((key, addresses)).build()
}
