//! Linode (Akamai): what its `cloud-vm` module ([`vm`]) and its `cloud-docker` module
//! ([`docker`]) share: the region and the firewall's rules.

pub(crate) mod docker;
pub(crate) mod vm;

use isoloom_render::hcl_builder::{Block, Expression, string};

use crate::module::variable::Variable;

/// The `region` variable: Paris by default.
pub(crate) fn region() -> Block {
    Variable::new("region", "string").default(string("fr-par")).block()
}

/// Which way a firewall rule applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Way {
    /// To what comes in, from its peers.
    In,
    /// To what goes out, to its peers.
    Out,
}

/// What a firewall rule does with what it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// Lets it through.
    Accept,
    /// Drops it.
    Drop,
}

/// A firewall rule, `label`, over `protocol` on `ports` (`None`: all of them), with `peers`.
pub(crate) fn rule((way, action): (Way, Action), label: &str, protocol: &str, ports: Option<String>, peers: Expression) -> Block {
    let kind = match way {
        Way::In => "inbound",
        Way::Out => "outbound",
    };
    let action = match action {
        Action::Accept => "ACCEPT",
        Action::Drop => "DROP",
    };
    let mut b = Block::builder(kind)
        .add_attribute(("label", string(label)))
        .add_attribute(("action", string(action)))
        .add_attribute(("protocol", string(protocol)));
    if let Some(p) = ports {
        b = b.add_attribute(("ports", string(p)));
    }
    b.add_attribute(("ipv4", peers)).build()
}
