//! IPv6 on a VM, written once for every VM target: each IPv6 address on the interface that
//! holds the machine's IPv4 address on that network (the providers configure IPv4 only), the
//! IPv6 routes through the router, the router's own IPv6 side, and its IPv6 forwarding rules.

use std::net::{Ipv4Addr, Ipv6Addr};

use isoloom_plan::Environment;
use isoloom_plan::ipv6::{Route6, block};
use isoloom_spec::model::Spec;
use isoloom_spec::shell::quote;
use isoloom_spec::validate::IPV6_PREFIX as PREFIX;

/// Shell that puts each `(IPv4 address, IPv6 address)` pair's IPv6 address on the interface
/// holding the IPv4 one, then installs the routes. Idempotent (`replace`); no duplicate address
/// detection, so the routes through the router work at once.
#[must_use]
pub fn linux(addresses: &[(Ipv4Addr, Ipv6Addr)], routes: &[Route6]) -> String {
    let on_interfaces = addresses.iter().map(|(v4, v6)| {
        let find = quote(&format!(" inet {v4}/"));
        format!(
            "dev=$(ip -o -4 addr show | grep -F {find} | awk '{{print $2; exit}}')\nsysctl -q -w \"net.ipv6.conf.$dev.disable_ipv6=0\"\nip -6 addr replace {v6}/{PREFIX} dev \"$dev\" nodad\n"
        )
    });
    let through_router = routes.iter().map(|r| format!("ip -6 route replace {} via {}\n", r.cidr, r.via));
    on_interfaces.chain(through_router).collect()
}

/// The same in PowerShell for a Windows VM: the interface found by its IPv4 address, each route
/// on the interface that reaches its next hop. Running it again adds nothing.
#[must_use]
pub fn windows(addresses: &[(Ipv4Addr, Ipv6Addr)], routes: &[Route6]) -> String {
    let mut lines: Vec<String> = addresses
        .iter()
        .map(|(v4, v6)| {
            format!(
                "if (-not (Get-NetIPAddress -IPAddress '{v6}' -ErrorAction SilentlyContinue)) {{ New-NetIPAddress -InterfaceIndex (Get-NetIPAddress -IPAddress '{v4}').InterfaceIndex -IPAddress '{v6}' -PrefixLength {PREFIX} | Out-Null }}"
            )
        })
        .collect();
    lines.extend(routes.iter().map(|r| {
        format!(
            "if (-not (Get-NetRoute -DestinationPrefix '{c}' -ErrorAction SilentlyContinue)) {{ New-NetRoute -DestinationPrefix '{c}' -InterfaceIndex (Find-NetRoute -RemoteIPAddress '{via}' | Select-Object -First 1).InterfaceIndex -NextHop '{via}' | Out-Null }}",
            c = r.cidr,
            via = r.via
        )
    }));
    lines.join("\n")
}

/// The router VM's IPv6 side: forwarding on, and its IPv6 address on each IPv6 network it routes
/// (`None` when it routes none). `addresses`: its `(IPv4, IPv6)` addresses there.
#[must_use]
pub fn router_linux(addresses: &[(Ipv4Addr, Ipv6Addr)]) -> Option<String> {
    (!addresses.is_empty()).then(|| format!("sysctl -q -w net.ipv6.conf.all.forwarding=1\n{}", linux(addresses, &[])))
}

/// The egress ruleset's IPv6 line, as one more quoted `printf` argument (empty without IPv6
/// networks): no new connection to an address outside the environment's IPv6 blocks.
#[must_use]
pub fn egress_rule(env: &Environment) -> String {
    let blocks: Vec<&str> = env.networks.values().filter_map(|n| n.ipv6.as_ref()).map(|v| v.cidr.as_str()).collect();
    if blocks.is_empty() {
        return String::new();
    }
    format!(" '    ip6 daddr != {{ {} }} ct state new drop'", blocks.join(", "))
}

/// The router's IPv6 rules for one `reach` rule (none unless both networks are IPv6), beside
/// the IPv4 ones in the same `inet` table: `ports` empty opens every port.
#[must_use]
pub fn forward_rules(spec: &Spec, from: &str, to: &str, ports: &[u16]) -> Vec<String> {
    let (Some(from), Some(to)) = (block(spec, from), block(spec, to)) else {
        return Vec::new();
    };
    let (from, to) = (from.text(), to.text());
    if ports.is_empty() {
        return vec![format!("ip6 saddr {from} ip6 daddr {to} accept")];
    }
    let ports = ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ");
    vec![
        format!("ip6 saddr {from} ip6 daddr {to} meta l4proto {{ tcp, udp }} th dport {{ {ports} }} accept"),
        format!("ip6 saddr {from} ip6 daddr {to} icmpv6 type echo-request accept"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    /// A machine's address on an IPv6 network, by its IPv4 one there.
    fn addresses() -> [(Ipv4Addr, Ipv6Addr); 1] {
        [("10.1.0.10".parse().unwrap(), "fd00:1::10".parse().unwrap())]
    }

    /// A route to another IPv6 network, through the router.
    fn routes() -> [Route6; 1] {
        [Route6 {
            cidr: "fd00:2::/64".into(),
            via: "fd00:1::254".parse().unwrap(),
        }]
    }

    #[test]
    fn the_address_lands_on_the_interface_of_the_ipv4_one() {
        let sh = linux(&addresses(), &routes());
        assert!(sh.contains("grep -F ' inet 10.1.0.10/'"), "{sh}");
        assert!(sh.contains("ip -6 addr replace fd00:1::10/64 dev \"$dev\" nodad"), "{sh}");
        assert!(sh.ends_with("ip -6 route replace fd00:2::/64 via fd00:1::254\n"), "{sh}");
        let ps = windows(&addresses(), &routes());
        assert_eq!(ps.lines().count(), 2, "{ps}");
        assert!(ps.contains("-IPAddress 'fd00:1::10' -PrefixLength 64"), "{ps}");
        assert!(
            ps.contains("New-NetRoute -DestinationPrefix 'fd00:2::/64'") && ps.contains("-NextHop 'fd00:1::254'"),
            "{ps}"
        );
    }

    #[test]
    fn the_router_forwards_ipv6_only_where_it_routes_an_ipv6_network() {
        assert_eq!(router_linux(&[]), None);
        let sh = router_linux(&[("10.1.0.254".parse().unwrap(), "fd00:1::254".parse().unwrap())]).expect("an IPv6 network");
        assert!(sh.starts_with("sysctl -q -w net.ipv6.conf.all.forwarding=1\ndev=$(ip"), "{sh}");
    }

    #[test]
    fn offline_drops_new_connections_outside_the_ipv6_blocks() {
        const SPEC: &str = "version: 1\nname: e\nnetworks:\n  a: { cidr: 10.1.0.0/24, ipv6: { cidr: \"fd00:1::/64\" } }\n  b: { cidr: 10.2.0.0/24 }\nmachines:\n  x: { networks: { a: 10, b: 10 }, os: debian@12 }\n";
        let env = testing::env(SPEC, isoloom_spec::model::Target::Vagrant);
        assert_eq!(egress_rule(&env), " '    ip6 daddr != { fd00:1::/64 } ct state new drop'");
        let v4 = testing::env(
            "version: 1\nname: e\nnetworks: { b: { cidr: 10.2.0.0/24 } }\nmachines:\n  x: { networks: { b: 10 }, os: debian@12 }\n",
            isoloom_spec::model::Target::Vagrant,
        );
        assert_eq!(egress_rule(&v4), "");
    }

    #[test]
    fn rules_only_between_two_ipv6_networks() {
        let spec = testing::spec(
            "version: 1\nname: r\nnetworks:\n  a: { cidr: 10.1.0.0/24, ipv6: { cidr: \"fd00:1::/64\" } }\n  b: { cidr: 10.2.0.0/24, ipv6: { cidr: \"fd00:2::/64\" } }\n  c: { cidr: 10.3.0.0/24 }\nmachines:\n  x: { networks: { a: 10 }, os: debian@12 }\n",
        );
        assert_eq!(forward_rules(&spec, "a", "b", &[]), vec!["ip6 saddr fd00:1::/64 ip6 daddr fd00:2::/64 accept"]);
        assert_eq!(forward_rules(&spec, "a", "b", &[80]).len(), 2);
        assert_eq!(forward_rules(&spec, "a", "c", &[]), Vec::<String>::new());
    }
}
