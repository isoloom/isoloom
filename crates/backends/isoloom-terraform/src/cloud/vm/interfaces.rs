//! A cloud VM's interfaces after its first: the cloud attaches one per network, and the
//! machine's set-up brings each up at its address, finding it by its MAC address.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Expression, Template};
use isoloom_spec::validate::Cidr;

/// The commands bringing up the interfaces after the first (found by MAC, `mac(network)` the
/// expression of its interface's MAC address, lowercase and colon-separated as `ip link`
/// writes it) at their addresses: at every boot, as the SSH user.
pub(crate) fn up(env: &Environment, addresses: &IndexMap<String, Ipv4Addr>, mac: &dyn Fn(&str) -> Expression) -> Vec<Template> {
    addresses
        .iter()
        .skip(1)
        .map(|(n, a)| {
            let len = Cidr::parse(&env.networks[n].cidr).map_or(32, |c| c.len);
            Template::text("IF=$(ip -o link | grep -i \"").with_expr(mac(n)).with_text(format!(
                "\" | awk -F': ' '{{print $2}}'); sudo ip link set \"$IF\" up && (ip -4 addr show \"$IF\" | grep -q {a}/ || sudo ip addr add {a}/{len} dev \"$IF\")"
            ))
        })
        .collect()
}
