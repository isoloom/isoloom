//! The router both generators add when a spec has `reach` rules: a machine on every network
//! at its last address, forwarding between networks and filtering with nftables. Machines
//! route the networks they aren't on through it.
//!
//! A network can instead name a machine of the environment as its `gateway` (an edge
//! firewall): the router stays off it, and its machines route through that gateway.

use std::net::Ipv4Addr;

use crate::model::{Machine, Spec};
use crate::validate::Cidr;

/// The router machine's name (a spec can't use it: machine names are user names, and this
/// one is prefixed).
pub const NAME: &str = "isoloom-router";

/// Whether Isoloom routes this network with its own router (no machine is its gateway).
pub fn plain(spec: &Spec, network: &str) -> bool {
    spec.networks[network].gateway.is_none()
}

/// Whether the spec needs a router: `reach` rules between networks it routes itself.
pub fn needed(spec: &Spec) -> bool {
    spec.reach.iter().any(|r| plain(spec, &r.from) && plain(spec, &r.to))
}

/// Whether traffic crosses networks at all (Isoloom's router, or a machine as gateway).
pub fn routed(spec: &Spec) -> bool {
    needed(spec) || spec.networks.values().any(|n| n.gateway.is_some())
}

/// The networks the router is on.
pub fn networks(spec: &Spec) -> impl Iterator<Item = &String> {
    spec.networks.keys().filter(|n| plain(spec, n))
}

fn cidr(spec: &Spec, network: &str) -> Cidr {
    Cidr::parse(&spec.networks[network].cidr).expect("validated cidr")
}

/// The router's address on a network.
pub fn address(spec: &Spec, network: &str) -> Ipv4Addr {
    cidr(spec, network).router()
}

/// Whether the machine is the gateway of one of its networks.
pub fn is_gateway(spec: &Spec, name: &str) -> bool {
    spec.networks.values().any(|n| n.gateway.as_deref() == Some(name))
}

/// Where a machine sends traffic for everywhere else (the internet included): the gateway
/// of its first network routed by another machine.
pub fn default_gateway(spec: &Spec, name: &str, m: &Machine) -> Option<Ipv4Addr> {
    m.networks
        .keys()
        .find(|n| spec.networks[*n].gateway.as_deref().is_some_and(|g| g != name))
        .map(|n| cidr(spec, n).gateway())
}

/// Routes a machine needs to the networks it isn't on: a network with a gateway through that
/// gateway (on a network they share), the others through the router (on the machine's first
/// network the router is on). Networks with neither route are left to the default route.
pub fn routes(spec: &Spec, name: &str, m: &Machine) -> Vec<(String, Ipv4Addr)> {
    let via_router = needed(spec)
        .then(|| m.networks.keys().find(|n| plain(spec, n)))
        .flatten()
        .map(|n| address(spec, n));
    spec.networks
        .iter()
        .filter(|(n, _)| !m.networks.contains_key(*n))
        .filter_map(|(_, net)| {
            let via = match &net.gateway {
                Some(gw) if gw != name => {
                    let g = &spec.machines[gw];
                    m.networks
                        .keys()
                        .find_map(|shared| g.networks.get(shared).map(|octet| super::address(spec, shared, *octet)))
                }
                Some(_) => None,
                None => via_router,
            }?;
            Some((net.cidr.clone(), via))
        })
        .collect()
}

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
    let lines: Vec<String> = networks(spec)
        .filter_map(|n| spec.networks[n].tc.as_ref().map(|t| (n, t)))
        .map(|(n, t)| {
            let addr = address(spec, n).to_string().replace('.', "\\.");
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
