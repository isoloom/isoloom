//! The tools beside the environment (`tools:`): observers on every network, outside its contract
//! (no `reach` rule, check or `resources` names them), each at the address reserved for it below
//! the controller's. The one place a tool's address is worked out, for the plan and the resolved
//! snapshot alike.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_spec::model::{Spec, Tool};

use crate::addressing::cidr;

/// The built-in `shell` tool's container image: a toolbox with tcpdump, nmap, curl, dig, netcat,
/// `tc` and more, pinned by the digest of its multi-arch index (what runs never changes under the
/// tag), as Isoloom's other helper images are.
pub const SHELL_IMAGE: &str = "nicolaka/netshoot:v0.14@sha256:7f08c4aff13ff61a35d30e30c5c1ea8396eac6ab4ce19fd02d5a4b3b5d0d09a2";

/// A tool, as the target lays it out.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolPlan {
    /// What it runs: its image, command and web UI's port, as the spec gives them.
    pub tool: Tool,
    /// The container image it runs: the spec's, else the built-in `shell`'s toolbox
    /// ([`SHELL_IMAGE`]; validation asks every other tool for an image). Resolved once here, so
    /// no backend or snapshot spells the toolbox again.
    pub image: String,
    /// Its address on each network, in the spec's order: the `i`th tool's reserved one
    /// ([`isoloom_spec::net::NetworkBlock::tool`]), on every network whose block has one for it.
    pub addresses: IndexMap<String, Ipv4Addr>,
}

/// The tools of `spec`, in its order, each at its addresses on the blocks `spec` lays out.
#[must_use]
pub fn of(spec: &Spec) -> IndexMap<String, ToolPlan> {
    spec.tools
        .iter()
        .enumerate()
        .map(|(i, (name, tool))| {
            let addresses = spec.networks.keys().filter_map(|net| Some((net.clone(), cidr(spec, net).tool(i)?))).collect();
            let image = tool.image.clone().unwrap_or_else(|| SHELL_IMAGE.to_owned());
            (
                name.clone(),
                ToolPlan {
                    tool: tool.clone(),
                    image,
                    addresses,
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use crate::testing;

    #[test]
    fn each_tool_takes_its_own_address_below_the_controller_s_on_every_network() {
        let spec = testing::spec(
            "version: 1\nname: t\nnetworks: { a: { cidr: 10.9.0.0/24 }, b: { cidr: 10.8.0.0/24 } }\nmachines:\n  web: { networks: { a: 10 }, os: debian@12 }\ntools: { shell: {}, ui: { image: x } }\n",
        );
        let tools = super::of(&spec);
        let at = |t: &str, n: &str| tools.get(t).and_then(|t| t.addresses.get(n)).copied();
        assert_eq!(at("shell", "a"), Some(Ipv4Addr::new(10, 9, 0, 252)));
        assert_eq!(at("ui", "a"), Some(Ipv4Addr::new(10, 9, 0, 251)));
        assert_eq!(at("ui", "b"), Some(Ipv4Addr::new(10, 8, 0, 251)));
        assert_eq!(tools["shell"].image, super::SHELL_IMAGE, "the built-in shell runs the toolbox");
        assert_eq!(tools["ui"].image, "x", "any other tool runs the image the spec gives");
    }

    #[test]
    fn the_shell_toolbox_is_pinned_by_a_digest() {
        let digest = super::SHELL_IMAGE.split_once("@sha256:").map(|(_, d)| d).unwrap_or_default();
        assert!(digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit()), "{}", super::SHELL_IMAGE);
        assert!(
            super::SHELL_IMAGE.split_once('@').is_some_and(|(name, _)| name.contains(':')),
            "a tag beside the digest"
        );
    }
}
