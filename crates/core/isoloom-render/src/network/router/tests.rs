use isoloom_spec::model::Machine;

use super::{forward_rules, machine_tc_script, nftables, tc_script};
use crate::testing;

/// Three networks: `a` reaches `b` on port 80, `b` reaches `c` on every port, `c` is slow, and
/// `edge` is routed by a machine of its own (no router rule goes there).
const SPEC: &str = r"
version: 1
name: r
networks:
  a: { cidr: 10.1.0.0/24 }
  b: { cidr: 10.2.0.0/24 }
  c: { cidr: 10.3.0.0/24, tc: { delay: 80ms } }
  edge: { cidr: 10.4.0.0/24, gateway: fw }
reach:
  - { from: a, to: b, ports: [80, 443] }
  - { from: b, to: c }
  - { from: a, to: edge }
machines:
  fw: { networks: { a: 5, edge: 1 }, app: { image: nginx } }
  web: { networks: { b: 10, c: 10 }, app: { image: nginx } }
";

#[test]
fn replies_then_each_reach_rule_between_routed_networks() {
    assert_eq!(
        forward_rules(&testing::spec(SPEC)),
        [
            "ct state established,related accept",
            "ip saddr 10.1.0.0/24 ip daddr 10.2.0.0/24 meta l4proto { tcp, udp } th dport { 80, 443 } accept",
            "ip saddr 10.1.0.0/24 ip daddr 10.2.0.0/24 icmp type echo-request accept",
            "ip saddr 10.2.0.0/24 ip daddr 10.3.0.0/24 accept",
        ]
    );
}

#[test]
fn the_ruleset_drops_what_no_rule_opens() {
    let spec = testing::spec(SPEC);
    let rules: String = forward_rules(&spec).iter().map(|r| ["    ", r, "\n"].concat()).collect();
    assert_eq!(
        nftables(&spec),
        format!("table inet isoloom {{\n  chain forward {{\n    type filter hook forward priority 0; policy drop;\n{rules}  }}\n}}\n"),
        "dropped unless a rule accepts it"
    );
}

#[test]
fn only_a_network_with_tc_impairs_its_interface() {
    let spec = testing::spec(SPEC);
    assert_eq!(
        tc_script(&spec).as_deref(),
        Some(
            "IF=$(ip -o -4 addr show | awk '$4 ~ /^10\\.3\\.0\\.254\\//{print $2}' | head -n 1); [ -n \"$IF\" ] && tc qdisc replace dev \"$IF\" root netem delay 80ms\n"
        ),
        "the router's interface on `c` alone"
    );
    let web: &Machine = &spec.machines["web"];
    let own = machine_tc_script(&spec, web, |n, o| isoloom_plan::addressing::address(&spec, n, o)).expect("`web` is on `c`");
    assert_eq!(
        own,
        "IF=$(ip -o -4 addr show | awk '$4 ~ /^10\\.3\\.0\\.10\\//{print $2}' | head -n 1); [ -n \"$IF\" ] && tc qdisc replace dev \"$IF\" root netem delay 80ms\n",
        "web's interface on `c` alone"
    );
    let calm =
        testing::spec("version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\n");
    assert_eq!(tc_script(&calm), None);
    assert_eq!(machine_tc_script(&calm, &calm.machines["web"], |_, _| std::net::Ipv4Addr::LOCALHOST), None);
}
