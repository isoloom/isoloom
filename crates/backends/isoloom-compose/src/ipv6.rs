//! IPv6 on Docker: a dual-stack network gets its IPv6 block beside the IPv4 one, an IPv6-only
//! network loses its IPv4 one (Docker Engine 28 or later), machines and the router get their
//! IPv6 addresses, and the network sidecars install the IPv6 routes through the router.

use std::net::{Ipv4Addr, Ipv6Addr};

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachinePlan, NetworkPlan};

use crate::file::{Attachment, Block, Network};

/// A network's Compose definition: `v4` its IPv4 block and Docker's address there, then its
/// IPv6 block when it has one (Docker at `::1`); an IPv6-only network keeps only that one.
pub(crate) fn network(n: &NetworkPlan, v4: Block, internal: bool) -> Network {
    let mut subnets = vec![v4];
    let mut net = Network {
        internal,
        ..Network::default()
    };
    if let Some(v6) = &n.ipv6 {
        net.enable_ipv6 = true;
        if v6.only {
            net.enable_ipv4 = Some(false);
            subnets.clear();
        }
        subnets.push(Block {
            subnet: v6.cidr.clone(),
            gateway: Some(v6.gateway.to_string()),
            ip_range: Some(dynamic_range(v6.gateway)),
        });
    }
    net.ipam = Some(crate::file::Ipam { config: subnets });
    net
}

/// Where Docker picks IPv6 addresses itself (a machine on `addresses: auto`, a check runner, a
/// tool): `<block>::1:0/112`, clear of the fixed addresses (`::2` to `::299`, the router's).
fn dynamic_range(gateway: Ipv6Addr) -> String {
    format!("{}/112", Ipv6Addr::from((u128::from(gateway) & !0xffff_ffff) | 0x1_0000))
}

/// An attachment to network `net` at IPv4 address `v4` (none on an IPv6-only network), with the
/// machine's IPv6 address there when it has a derived one.
pub(crate) fn attachment(env: &Environment, mp: &MachinePlan, net: &str, v4: Ipv4Addr) -> Attachment {
    Attachment {
        ipv4_address: (!only(env, net)).then(|| v4.to_string()),
        ipv6_address: mp.addresses6.get(net).map(ToString::to_string),
        ..Attachment::default()
    }
}

/// Whether the network carries no IPv4.
pub(crate) fn only(env: &Environment, net: &str) -> bool {
    env.networks.get(net).and_then(|n| n.ipv6.as_ref()).is_some_and(|v| v.only)
}

/// Whether the machine is on an IPv6 network (it has an IPv6 default route to drop when
/// offline).
pub(crate) fn on_ipv6(env: &Environment, mp: &MachinePlan) -> bool {
    mp.addresses.keys().any(|n| env.networks.get(n).is_some_and(|p| p.ipv6.is_some()))
}

/// The router's attachment to `net` at `v4`, with its IPv6 address when it routes IPv6 there.
pub(crate) fn router_attachment(env: &Environment, net: &str, v4: Ipv4Addr) -> Attachment {
    Attachment {
        ipv4_address: Some(v4.to_string()),
        ipv6_address: env.router6.get(net).map(ToString::to_string),
        ..Attachment::default()
    }
}

/// The router's sysctls: IPv4 forwarding, and IPv6 forwarding when it routes IPv6.
pub(crate) fn forwarding(env: &Environment) -> IndexMap<String, String> {
    let mut out = IndexMap::from([("net.ipv4.ip_forward".to_owned(), "1".to_owned())]);
    if !env.router6.is_empty() {
        out.insert("net.ipv6.conf.all.forwarding".into(), "1".into());
    }
    out
}

/// The sidecar's commands for a machine's IPv6 routes (`ip -6 route replace`, idempotent).
pub(crate) fn route_commands(mp: &MachinePlan) -> impl Iterator<Item = String> + '_ {
    mp.routes6.iter().map(|r| format!("ip -6 route replace {} via {}", r.cidr, r.via))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::environment;

    const SPEC: &str = "version: 1
name: v6
networks:
  dual: { cidr: 10.1.0.0/24, ipv6: { cidr: \"fd00:1::/64\" } }
  only: { cidr: 10.2.0.0/24, internet: false, ipv6: { cidr: \"fd00:2::/64\", only: true } }
machines:
  web: { networks: { dual: 10, only: 10 }, docker: { image: nginx } }
";

    #[test]
    fn dual_stack_keeps_both_blocks_and_ipv6_only_drops_ipv4() {
        let env = environment(SPEC);
        let v4 = |n: &str| Block {
            subnet: env.networks[n].cidr.clone(),
            gateway: None,
            ip_range: None,
        };
        let dual = network(&env.networks["dual"], v4("dual"), false);
        assert!(dual.enable_ipv6 && dual.enable_ipv4.is_none());
        let config = dual.ipam.unwrap().config;
        assert_eq!(config[1].ip_range.as_deref(), Some("fd00:1::1:0/112"));
        let only = network(&env.networks["only"], v4("only"), true);
        assert_eq!(only.enable_ipv4, Some(false));
        assert_eq!(only.ipam.unwrap().config[0].subnet, "fd00:2::/64");
        let mp = &env.machines["web"];
        let a = attachment(&env, mp, "only", mp.addresses["only"]);
        assert_eq!((a.ipv4_address, a.ipv6_address.as_deref()), (None, Some("fd00:2::10")));
    }
}
