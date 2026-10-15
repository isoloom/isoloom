//! Networks outside the environment: an existing one joined by name (`external`), or one
//! bridged to the host's interface (`bridged`). Isoloom doesn't make, route or impair them, and
//! a machine is reached on one of the environment's own networks, so each machine lists one of
//! those first.

use super::Problems;
use crate::model::{Network, Spec};

/// Each outside network: one kind, a name or interface that reads as one, and none of the
/// settings that only Isoloom's own networks take.
pub(super) fn networks(spec: &Spec, p: &mut Problems) {
    for (name, net) in &spec.networks {
        let at = format!("networks.{name}");
        if net.external.is_some() && net.bridged.is_some() {
            p.add(
                &at,
                "a network is either `external` (an existing network, joined by name) or `bridged` (to a host interface), not both",
            );
        }
        if let Some(e) = &net.external
            && !word(e)
        {
            p.add(format!("{at}.external"), "the existing network's name, like `corp-net` or `vmbr1` (no spaces)");
        }
        if let Some(b) = &net.bridged {
            if b.interface.as_deref().is_some_and(|i| i.trim().is_empty()) {
                p.add(format!("{at}.bridged.interface"), "name the host interface, or leave `interface` out");
            }
            if b.default_route && !b.dhcp {
                p.add(
                    format!("{at}.bridged.default_route"),
                    "the default route comes from the LAN's DHCP server: set `dhcp: true`, or leave `default_route` out",
                );
            }
            if b.vlan.is_some_and(|v| !(1..=4094).contains(&v)) {
                p.add(format!("{at}.bridged.vlan"), "a VLAN tag is 1 to 4094");
            }
        }
        if net.is_outside() {
            own_settings(&at, name, net, p);
        }
    }
}

/// A name without spaces (a Docker network's, a Proxmox bridge's).
fn word(s: &str) -> bool {
    !s.is_empty() && !s.chars().any(char::is_whitespace)
}

/// What only the environment's own networks take: Isoloom routes, impairs and lays them out.
fn own_settings(at: &str, name: &str, net: &Network, p: &mut Problems) {
    let why = format!("`{name}` is outside the environment: its own routing decides");
    if net.gateway.is_some() {
        p.add(format!("{at}.gateway"), format!("{why}, so leave `gateway` out"));
    }
    if !net.internet {
        p.add(format!("{at}.internet"), format!("{why}, so leave `internet` out"));
    }
    if net.tc.is_some() {
        p.add(format!("{at}.tc"), format!("{why}, so leave `tc` out (it applies on Isoloom's router)"));
    }
    if net.docker.is_some() {
        p.add(
            format!("{at}.docker"),
            format!("`{name}` keeps its own block on Docker: leave its `cidr` out of `overrides.docker`"),
        );
    }
    if net.switch.is_some() {
        p.add(format!("{at}.switch"), format!("{why}, so leave `switch` out"));
    }
    if net.ipv6.is_some() {
        p.add(
            format!("{at}.ipv6"),
            format!("`{name}` is outside the environment: its IPv6 is its own, so leave `ipv6` out"),
        );
    }
}

/// No `reach` rule names an outside network: Isoloom's router isn't on it.
pub(super) fn reach(spec: &Spec, p: &mut Problems) {
    for (i, r) in spec.reach.iter().enumerate() {
        for (field, net) in [("from", &r.from), ("to", &r.to)] {
            if spec.networks.get(net).is_some_and(Network::is_outside) {
                p.add(
                    format!("reach[{i}].{field}"),
                    format!("`{net}` is outside the environment: Isoloom's router isn't on it, so its own routing decides"),
                );
            }
        }
    }
}

/// Each machine's first network is one of the environment's own: it is reached there.
pub(super) fn first_networks(spec: &Spec, p: &mut Problems) {
    for (name, m) in &spec.machines {
        let Some((first, _)) = m.networks.first() else { continue };
        if spec.networks.get(first).is_some_and(Network::is_outside) {
            p.add(
                format!("machines.{name}.networks"),
                format!("`{first}` is outside the environment: list one of the environment's own networks first (a machine is reached on its first network)"),
            );
        }
    }
}

/// A machine's address on network `net` when DHCP decides it or `octet` asks for DHCP: `0`, on
/// a `bridged: { dhcp: true }` network only. True when this settles the address (the caller
/// checks a fixed one otherwise).
pub(super) fn dhcp_address(spec: &Spec, net: &str, octet: u8, at: &str, p: &mut Problems) -> bool {
    let dhcp = spec.networks.get(net).is_some_and(Network::is_dhcp);
    match (dhcp, octet) {
        (true, 0) => {}
        (true, _) => p.add(at, format!("`{net}` gives addresses by DHCP: write `{net}: 0`")),
        (false, 0) => p.add(at, "0 asks for an address by DHCP, on a `bridged: { dhcp: true }` network only"),
        (false, _) => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::testing::model as parse;
    use crate::validate;

    fn problems(yaml: &str) -> Vec<String> {
        validate(&parse(yaml).unwrap()).into_iter().map(|p| p.to_string()).collect()
    }

    const BASE: &str = "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.20.0.0/24 }\n";

    #[test]
    fn an_outside_network_is_valid_after_an_own_one() {
        let spec = format!(
            "{BASE}  corp: {{ cidr: 192.168.50.0/24, external: corp-net }}\n  home: {{ cidr: 10.0.0.0/24, bridged: {{ interface: 'en0: Wi-Fi', dhcp: true, default_route: true }} }}\nmachines:\n  a: {{ networks: {{ lan: 10, corp: 10, home: 0 }}, docker: {{ image: nginx }} }}\n"
        );
        assert_eq!(problems(&spec), Vec::<String>::new());
    }

    #[test]
    fn outside_networks_refuse_what_only_own_networks_take() {
        let spec = format!(
            "{BASE}  corp: {{ cidr: 192.168.50.0/24, external: corp-net, bridged: {{ default_route: true }}, internet: false, tc: {{ delay: 5ms }} }}\nreach: [{{ from: corp, to: lan }}]\nmachines:\n  a: {{ networks: {{ corp: 10, lan: 0 }}, docker: {{ image: nginx }} }}\n"
        );
        let got = problems(&spec);
        for want in [
            "networks.corp: a network is either `external`",
            "networks.corp.bridged.default_route: the default route comes from the LAN's DHCP server",
            "networks.corp.internet: `corp` is outside the environment",
            "networks.corp.tc: `corp` is outside the environment",
            "reach[0].from: `corp` is outside the environment",
            "machines.a.networks: `corp` is outside the environment: list one of the environment's own networks first",
            "machines.a.networks.lan: 0 asks for an address by DHCP",
        ] {
            assert!(got.iter().any(|g| g.starts_with(want)), "missing {want:?} in {got:#?}");
        }
    }

    #[test]
    fn a_dhcp_network_takes_zero_and_a_name_has_no_spaces() {
        let spec = format!(
            "{BASE}  home: {{ cidr: 10.0.0.0/24, bridged: {{ dhcp: true }} }}\n  corp: {{ cidr: 10.30.0.0/24, external: 'corp net' }}\nmachines:\n  a: {{ networks: {{ lan: 10, home: 10 }}, docker: {{ image: nginx }} }}\n"
        );
        let got = problems(&spec);
        assert!(
            got.contains(&"machines.a.networks.home: `home` gives addresses by DHCP: write `home: 0`".to_owned()),
            "{got:#?}"
        );
        assert!(
            got.iter().any(|g| g.starts_with("networks.corp.external: the existing network's name")),
            "{got:#?}"
        );
    }
}
