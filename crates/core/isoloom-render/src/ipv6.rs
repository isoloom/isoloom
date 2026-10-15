//! IPv6 on a VM, written once for every VM target: each IPv6 address on the interface that
//! holds the machine's IPv4 address on that network (the providers configure IPv4 only), the
//! IPv6 routes through the router, the router's own IPv6 side, and its IPv6 forwarding rules.

use std::fmt::Write as _;
use std::net::{Ipv4Addr, Ipv6Addr};

use isoloom_plan::Environment;
use isoloom_plan::ipv6::{Route6, block};
use isoloom_spec::validate::IPV6_PREFIX as PREFIX;

use crate::model::Spec;
use crate::shell::quote;

/// Shell that puts each `(IPv4 address, IPv6 address)` pair's IPv6 address on the interface
/// holding the IPv4 one, then installs the routes. Idempotent (`replace`); no duplicate address
/// detection, so the routes through the router work at once.
pub fn linux(addresses: &[(Ipv4Addr, Ipv6Addr)], routes: &[Route6]) -> String {
    let mut sh = String::new();
    for (v4, v6) in addresses {
        let find = quote(&format!(" inet {v4}/"));
        let _ = write!(
            sh,
            "dev=$(ip -o -4 addr show | grep -F {find} | awk '{{print $2; exit}}')\nsysctl -q -w \"net.ipv6.conf.$dev.disable_ipv6=0\"\nip -6 addr replace {v6}/{PREFIX} dev \"$dev\" nodad\n"
        );
    }
    for r in routes {
        let _ = writeln!(sh, "ip -6 route replace {} via {}", r.cidr, r.via);
    }
    sh
}

/// The same in PowerShell for a Windows VM: the interface found by its IPv4 address, each route
/// on the interface that reaches its next hop. Running it again adds nothing.
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
pub fn router_linux(addresses: &[(Ipv4Addr, Ipv6Addr)]) -> Option<String> {
    (!addresses.is_empty()).then(|| format!("sysctl -q -w net.ipv6.conf.all.forwarding=1\n{}", linux(addresses, &[])))
}

/// The egress ruleset's IPv6 line, as one more quoted `printf` argument (empty without IPv6
/// networks): no new connection to an address outside the environment's IPv6 blocks.
pub fn egress_rule(env: &Environment) -> String {
    let blocks: Vec<&str> = env.networks.values().filter_map(|n| n.ipv6.as_ref()).map(|v| v.cidr.as_str()).collect();
    if blocks.is_empty() {
        return String::new();
    }
    format!(" '    ip6 daddr != {{ {} }} ct state new drop'", blocks.join(", "))
}

/// The router's IPv6 rules for one `reach` rule (none unless both networks are IPv6), beside
/// the IPv4 ones in the same `inet` table: `ports` empty opens every port.
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

    #[test]
    fn the_address_lands_on_the_interface_of_the_ipv4_one() {
        let sh = linux(
            &[("10.1.0.10".parse().unwrap(), "fd00:1::10".parse().unwrap())],
            &[Route6 {
                cidr: "fd00:2::/64".into(),
                via: "fd00:1::254".parse().unwrap(),
            }],
        );
        assert!(sh.contains("grep -F ' inet 10.1.0.10/'"), "{sh}");
        assert!(sh.contains("ip -6 addr replace fd00:1::10/64 dev \"$dev\" nodad"), "{sh}");
        assert!(sh.ends_with("ip -6 route replace fd00:2::/64 via fd00:1::254\n"), "{sh}");
        let ps = windows(&[("10.1.0.10".parse().unwrap(), "fd00:1::10".parse().unwrap())], &[]);
        assert!(ps.contains("-IPAddress 'fd00:1::10' -PrefixLength 64"), "{ps}");
    }

    #[test]
    fn rules_only_between_two_ipv6_networks() {
        let spec = isoloom_spec::parse(
            "version: 1\nname: r\nnetworks:\n  a: { cidr: 10.1.0.0/24, ipv6: { cidr: \"fd00:1::/64\" } }\n  b: { cidr: 10.2.0.0/24, ipv6: { cidr: \"fd00:2::/64\" } }\n  c: { cidr: 10.3.0.0/24 }\nmachines: {}\n",
        )
        .unwrap();
        assert_eq!(forward_rules(&spec, "a", "b", &[]), vec!["ip6 saddr fd00:1::/64 ip6 daddr fd00:2::/64 accept"]);
        assert_eq!(forward_rules(&spec, "a", "b", &[80]).len(), 2);
        assert_eq!(forward_rules(&spec, "a", "c", &[]), Vec::<String>::new());
    }
}
