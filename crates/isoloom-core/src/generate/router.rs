//! The router both generators add when a spec has `reach` rules: a machine on every network
//! at its last address, forwarding between networks and filtering with nftables. Machines
//! route the networks they aren't on through it.

use std::net::Ipv4Addr;

use crate::model::{Machine, Spec};
use crate::validate::Cidr;

/// The router machine's name (a spec can't use it: machine names are user names, and this
/// one is prefixed).
pub const NAME: &str = "isoloom-router";

/// Whether the spec needs a router.
pub fn needed(spec: &Spec) -> bool {
    !spec.reach.is_empty()
}

fn cidr(spec: &Spec, network: &str) -> Cidr {
    Cidr::parse(&spec.networks[network].cidr).expect("validated cidr")
}

/// The router's address on a network.
pub fn address(spec: &Spec, network: &str) -> Ipv4Addr {
    cidr(spec, network).router()
}

/// Routes a machine needs: every network it isn't on, via the router on its first network.
pub fn routes(spec: &Spec, m: &Machine) -> Vec<(String, Ipv4Addr)> {
    let Some((first, _)) = m.networks.first() else { return Vec::new() };
    let via = address(spec, first);
    spec.networks
        .iter()
        .filter(|(n, _)| !m.networks.contains_key(*n))
        .map(|(_, net)| (net.cidr.clone(), via))
        .collect()
}

/// Shell commands that install those routes (`ip route replace` is idempotent).
pub fn route_commands(spec: &Spec, m: &Machine) -> Vec<String> {
    routes(spec, m)
        .into_iter()
        .map(|(cidr, via)| format!("ip route replace {cidr} via {via}"))
        .collect()
}

/// The nftables ruleset: forwarding between networks is dropped, except what `reach` allows
/// (and replies to it).
pub fn nftables(spec: &Spec) -> String {
    let mut rules = vec!["ct state established,related accept".to_string()];
    for r in &spec.reach {
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
