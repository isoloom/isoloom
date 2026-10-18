//! The router both kinds of target add when a spec has `reach` rules: a machine on every network
//! at its last address, forwarding between networks and filtering with nftables. Machines route
//! the networks they aren't on through it.
//!
//! A network can instead name a machine of the environment as its `gateway` (an edge firewall):
//! the router stays off it, and its machines route through that gateway.
//!
//! The facts (where the router stands, which networks it routes) are the plan's
//! (`isoloom_plan::routing`), re-exported here beside what is rendered from them, so a backend
//! reads the router in one place.

use std::net::Ipv4Addr;

pub use isoloom_plan::routing::{NAME, address, default_gateway, is_gateway, needed, networks, plain, routed, routes};
use isoloom_spec::model::{Machine, Spec};

use super::ipv6;

/// The nftables ruleset: forwarding between networks is dropped, except what `reach` allows
/// (and replies to it).
#[must_use]
pub fn nftables(spec: &Spec) -> String {
    let body = forward_rules(spec).iter().map(|r| format!("    {r}")).collect::<Vec<_>>().join("\n");
    format!("table inet isoloom {{\n  chain forward {{\n    type filter hook forward priority 0; policy drop;\n{body}\n  }}\n}}\n")
}

/// The rules of the router's forward chain: replies, then what each `reach` rule between two
/// networks it routes opens.
#[must_use]
pub fn forward_rules(spec: &Spec) -> Vec<String> {
    let mut rules = vec!["ct state established,related accept".to_owned()];
    for r in spec.reach.iter().filter(|r| plain(spec, &r.from) && plain(spec, &r.to)) {
        let (Some(from), Some(to)) = (spec.networks.get(&r.from), spec.networks.get(&r.to)) else {
            continue;
        };
        let (from, to) = (&from.cidr, &to.cidr);
        if r.ports.is_empty() {
            rules.push(format!("ip saddr {from} ip daddr {to} accept"));
        } else {
            let ports = r.ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ");
            rules.push(format!(
                "ip saddr {from} ip daddr {to} meta l4proto {{ tcp, udp }} th dport {{ {ports} }} accept"
            ));
            rules.push(format!("ip saddr {from} ip daddr {to} icmp type echo-request accept"));
        }
        rules.extend(ipv6::forward_rules(spec, &r.from, &r.to, &r.ports));
    }
    rules
}

/// Shell that applies each network's `tc` on the router's interface into it (found by the
/// router's address there), or `None` when no network asks for it. Idempotent (`replace`).
#[must_use]
pub fn tc_script(spec: &Spec) -> Option<String> {
    netem_script(spec, networks(spec).map(|n| (n.as_str(), address(spec, n))))
}

/// The same on a machine's own interfaces (`addr`: its address on a network), so traffic
/// between machines of one network (a direct cable, a LAN) is impaired too, not only what the
/// router forwards into it: each end delays what it sends, `delay` is one-way.
#[must_use]
pub fn machine_tc_script(spec: &Spec, m: &Machine, addr: impl Fn(&str, u8) -> Ipv4Addr) -> Option<String> {
    netem_script(spec, m.networks.iter().map(|(n, octet)| (n.as_str(), addr(n, *octet))))
}

/// One line per interface (found by its address) on a network with `tc`.
fn netem_script<'a>(spec: &Spec, at: impl Iterator<Item = (&'a str, Ipv4Addr)>) -> Option<String> {
    let lines: Vec<String> = at
        .filter_map(|(n, a)| spec.networks.get(n).and_then(|net| net.tc.as_ref()).map(|t| (a, t)))
        .map(|(a, t)| {
            let addr = a.to_string().replace('.', "\\.");
            format!(
                "IF=$(ip -o -4 addr show | awk '$4 ~ /^{addr}\\//{{print $2}}' | head -n 1); [ -n \"$IF\" ] && tc qdisc replace dev \"$IF\" root netem {}",
                t.netem()
            )
        })
        .collect();
    (!lines.is_empty()).then(|| format!("{}\n", lines.join("\n")))
}

#[cfg(test)]
mod tests;
