//! The tools beside the environment (`tools:`): observers on every network, outside its contract
//! (no `reach` rule, check or `resources` names them), each at the address reserved for it below
//! the controller's. The one place a tool's address is worked out, for the plan and the resolved
//! snapshot alike.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_spec::model::{Spec, Tool};

use crate::addressing::cidr;

/// A tool, as the target lays it out.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolPlan {
    /// What it runs: its image, command and web UI's port, as the spec gives them.
    pub tool: Tool,
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
            (name.clone(), ToolPlan { tool: tool.clone(), addresses })
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
    }
}
