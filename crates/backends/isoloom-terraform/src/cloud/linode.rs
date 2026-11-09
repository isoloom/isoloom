//! Linode (Akamai): what its `cloud-vm` module ([`vm`]) and its `cloud-docker` module
//! ([`docker`]) share: the region and the firewall's rules.

pub(crate) mod docker;
pub(crate) mod vm;

use isoloom_render::hcl_builder::{Block, Expression, attr, call, index, reference, string};

use super::{Base, operator};
use crate::cloud::instance;
use crate::module::variable::Variable;

/// What Linode's modules share: its token and who may reach the machines, the region, the
/// provider (its token read from the environment), and the user its images sign in as.
pub(crate) const BASE: Base = Base {
    usage: |folder| operator::with_token(folder, "# Auth:   export LINODE_TOKEN=<token> (the provider reads it).\n"),
    variables: || vec![region()],
    provider: |_, m| {
        operator::bare_provider(m, "linode");
        Vec::new()
    },
    user: |_| vm::USER,
};

/// The `region` variable: Paris by default.
pub(crate) fn region() -> Variable {
    Variable::new("region", "string").default(string("fr-par"))
}

/// An instance's `metadata` block: its user data (base64), which cloud-init reads from Linode's
/// metadata service.
pub(crate) fn metadata(user_data: Expression) -> Block {
    Block::builder("metadata")
        .add_attribute(("user_data", call("base64encode", [user_data])))
        .build()
}

/// An instance's `lifecycle`: set up once from its user data ([`instance::set_up_once`]), which the
/// provider would otherwise replace the instance for (`metadata.user_data` is `ForceNew` in the
/// `linode/linode` provider the modules require).
pub(crate) fn set_up_once() -> Block {
    instance::set_up_once(attr(index(reference("metadata"), 0), "user_data")).build()
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
