//! The router both generators add when a spec has `reach` rules: a machine on every network
//! at its last address, forwarding between networks and filtering with nftables. Machines
//! route the networks they aren't on through it.
//!
//! A network can instead name a machine of the environment as its `gateway` (an edge
//! firewall): the router stays off it, and its machines route through that gateway.

use std::net::Ipv4Addr;

pub use isoloom_plan::routing::{NAME, address, default_gateway, is_gateway, needed, networks, plain, routed, routes};

use crate::model::{Machine, Spec};

/// Shell commands that install those routes, then the default route through a gateway
/// (`ip route replace` is idempotent).
pub fn route_commands(spec: &Spec, name: &str, m: &Machine, with_default: bool) -> Vec<String> {
    let mut cmds: Vec<String> = routes(spec, name, m)
        .into_iter()
        .map(|(cidr, via)| format!("ip route replace {cidr} via {via}"))
        .collect();
    if with_default && let Some(gw) = default_gateway(spec, name, m) {
        cmds.push(format!("ip route replace default via {gw}"));
    }
    cmds
}

/// The nftables ruleset: forwarding between networks is dropped, except what `reach` allows
/// (and replies to it).
pub fn nftables(spec: &Spec) -> String {
    let mut rules = vec!["ct state established,related accept".to_string()];
    for r in spec.reach.iter().filter(|r| plain(spec, &r.from) && plain(spec, &r.to)) {
        let from = &spec.networks[&r.from].cidr;
        let to = &spec.networks[&r.to].cidr;
        if r.ports.is_empty() {
            rules.push(format!("ip saddr {from} ip daddr {to} accept"));
        } else {
            let ports = r.ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ");
            rules.push(format!(
                "ip saddr {from} ip daddr {to} meta l4proto {{ tcp, udp }} th dport {{ {ports} }} accept"
            ));
            rules.push(format!("ip saddr {from} ip daddr {to} icmp type echo-request accept"));
        }
    }
    let body = rules.iter().map(|r| format!("    {r}")).collect::<Vec<_>>().join("\n");
    format!("table inet isoloom {{\n  chain forward {{\n    type filter hook forward priority 0; policy drop;\n{body}\n  }}\n}}\n")
}

/// Shell that applies each network's `tc` on the router's interface into it (found by the
/// router's address there), or `None` when no network asks for it. Idempotent (`replace`).
pub fn tc_script(spec: &Spec) -> Option<String> {
    netem_script(spec, networks(spec).map(|n| (n.as_str(), address(spec, n))))
}

/// The same on a machine's own interfaces (`addr`: its address on a network), so traffic
/// between machines of one network (a direct cable, a LAN) is impaired too, not only what the
/// router forwards into it: each end delays what it sends, `delay` is one-way.
pub fn machine_tc_script(spec: &Spec, m: &Machine, addr: impl Fn(&str, u8) -> Ipv4Addr) -> Option<String> {
    netem_script(spec, m.networks.iter().map(|(n, octet)| (n.as_str(), addr(n, *octet))))
}

/// One line per interface (found by its address) on a network with `tc`.
fn netem_script<'a>(spec: &Spec, at: impl Iterator<Item = (&'a str, Ipv4Addr)>) -> Option<String> {
    let lines: Vec<String> = at
        .filter_map(|(n, a)| spec.networks[n].tc.as_ref().map(|t| (a, t)))
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

/// A shell loop that waits until `host` answers on every port (bash's /dev/tcp, then nc),
/// giving up after `timeout` seconds.
pub fn wait_for(host: &str, ports: &[u16], timeout: u32) -> String {
    let probes = ports
        .iter()
        .map(|p| format!("(bash -c '</dev/tcp/{host}/{p}' 2>/dev/null || nc -z -w 2 {host} {p} 2>/dev/null)"))
        .collect::<Vec<_>>()
        .join(" && ");
    format!(
        "i=0; until {probes}; do i=$((i+2)); if [ $i -ge {timeout} ]; then echo \"{host} didn't answer within {timeout}s\" >&2; exit 1; fi; sleep 2; done; echo \"{host} answers\""
    )
}
