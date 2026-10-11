//! VLANs written under their LAN, turned into plain networks.
//!
//! A spec may split a LAN into VLANs (`networks.office.vlans.10`) and attach machines to
//! `office.vlan10`. Every target already knows how to build separate networks, so the VLANs
//! become networks of their own here, once, right after parsing: `office-vlan10`, with the
//! machines' attachments and the `reach` rules rewritten to match. The LAN itself stays a
//! network only if a machine joins it directly (its untagged part); otherwise it was only the
//! frame around its VLANs. Generators and validation never see `vlans`.

use indexmap::IndexMap;

use crate::model::{Network, Reach, Spec};
use crate::validate::Cidr;

/// The network a VLAN becomes: `office` + 10 -> `office-vlan10`.
pub fn vlan_network(lan: &str, id: u16) -> String {
    format!("{lan}-vlan{id}")
}

/// The VLAN networks of a LAN, in order: (network, VLAN id). After [`flatten`].
pub fn of_lan<'a>(spec: &'a Spec, lan: &'a str) -> impl Iterator<Item = (&'a String, u16)> + 'a {
    spec.networks
        .iter()
        .filter_map(move |(n, net)| net.vlan.as_ref().filter(|(l, _)| l == lan).map(|(_, id)| (n, *id)))
}

/// The machine that switches a LAN (its `switch`), if any. After [`flatten`].
pub fn switch_of<'a>(spec: &'a Spec, lan: &str) -> Option<&'a str> {
    spec.networks
        .values()
        .find(|n| n.vlan.as_ref().is_some_and(|(l, _)| l == lan))
        .and_then(|n| n.switch.as_deref())
}

/// The LAN a machine switches (it is that LAN's `switch`), if any. After [`flatten`].
pub fn switched_by<'a>(spec: &'a Spec, machine: &str) -> Option<&'a str> {
    spec.networks
        .values()
        .find(|n| n.vlan.is_some() && n.switch.as_deref() == Some(machine))
        .and_then(|n| n.vlan.as_ref())
        .map(|(lan, _)| lan.as_str())
}

/// `office.vlan10` -> (`office`, 10).
fn vlan_ref(name: &str) -> Option<(&str, u16)> {
    let (lan, vlan) = name.split_once('.')?;
    let id: u16 = vlan.strip_prefix("vlan")?.parse().ok()?;
    Some((lan, id))
}

/// The spec with every LAN's VLANs as networks of their own (see the module).
pub fn flatten(mut spec: Spec) -> Result<Spec, String> {
    if spec.networks.values().all(|n| n.vlans.is_empty()) {
        return Ok(spec);
    }
    // Checks first: ids, blocks inside the LAN, no clash with a network already named so.
    for (lan, net) in &spec.networks {
        if net.vlans.is_empty() {
            continue;
        }
        if net.gateway.is_some() {
            return Err(format!(
                "networks.{lan}: a LAN split into VLANs can't have a `gateway` (set the gateway machine on each VLAN's network instead)"
            ));
        }
        let outer = Cidr::parse(&net.cidr);
        for (id, vlan) in &net.vlans {
            let at = format!("networks.{lan}.vlans.{id}");
            if !(1..=4094).contains(id) {
                return Err(format!("{at}: a VLAN id is between 1 and 4094"));
            }
            let inner = Cidr::parse(&vlan.cidr).ok_or_else(|| format!("{at}.cidr: `{}` isn't an IPv4 block such as 10.10.10.0/24", vlan.cidr))?;
            if let Some(outer) = outer
                && !outer.contains(inner)
            {
                return Err(format!("{at}.cidr: {} isn't inside the LAN's block {}", vlan.cidr, net.cidr));
            }
            let flat = vlan_network(lan, *id);
            if spec.networks.contains_key(&flat) {
                return Err(format!("{at}: it becomes the network `{flat}`, which the spec already has"));
            }
        }
    }

    // Machines: `office.vlan10` -> `office-vlan10`.
    for m in spec.machines.values_mut() {
        m.networks = m
            .networks
            .iter()
            .map(|(name, octet)| {
                let flat = match vlan_ref(name) {
                    Some((lan, id)) if spec.networks.get(lan).is_some_and(|n| n.vlans.contains_key(&id)) => vlan_network(lan, id),
                    _ => name.clone(),
                };
                (flat, *octet)
            })
            .collect();
    }
    let joined_directly = |lan: &str| spec.machines.values().any(|m| m.networks.contains_key(lan));

    // The networks, in the order written: each LAN with VLANs replaced by its VLANs (after the
    // LAN itself, when machines join it directly).
    let mut networks: IndexMap<String, Network> = IndexMap::new();
    // What a name in `reach` covers.
    let mut covers: IndexMap<String, Vec<String>> = IndexMap::new();
    for (lan, net) in &spec.networks {
        if net.vlans.is_empty() {
            networks.insert(lan.clone(), net.clone());
            continue;
        }
        let mut all = Vec::new();
        if joined_directly(lan) {
            networks.insert(
                lan.clone(),
                Network {
                    vlans: IndexMap::new(),
                    ..net.clone()
                },
            );
            all.push(lan.clone());
        }
        for (id, vlan) in &net.vlans {
            let flat = vlan_network(lan, *id);
            networks.insert(
                flat.clone(),
                Network {
                    cidr: vlan.cidr.clone(),
                    internet: vlan.internet.unwrap_or(net.internet),
                    gateway: None,
                    docker: None,
                    vlans: IndexMap::new(),
                    // The LAN's switch, on each of its VLANs (the LAN itself is gone).
                    switch: net.switch.clone(),
                    // A VLAN carries no impairment of its own (set it on a network the router is on).
                    tc: None,
                    vlan: Some((lan.clone(), *id)),
                },
            );
            covers.insert(format!("{lan}.vlan{id}"), vec![flat.clone()]);
            all.push(flat);
        }
        covers.insert(lan.clone(), all);
    }

    let expand = |name: &str| covers.get(name).cloned().unwrap_or_else(|| vec![name.to_string()]);
    let mut reach: Vec<Reach> = Vec::new();
    for r in &spec.reach {
        for from in expand(&r.from) {
            for to in expand(&r.to) {
                if from != to {
                    reach.push(Reach {
                        from: from.clone(),
                        to,
                        ports: r.ports.clone(),
                    });
                }
            }
        }
    }

    spec.networks = networks;
    spec.reach = reach;
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(yaml: &str) -> Spec {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    const OFFICE: &str = "version: 1
name: office
networks:
  office:
    cidr: 10.10.0.0/16
    internet: false
    vlans:
      10: { cidr: 10.10.10.0/24 }
      20: { cidr: 10.10.20.0/24, internet: true }
  servers: { cidr: 10.20.0.0/24 }
reach:
  - { from: office, to: servers, ports: [443] }
  - { from: office.vlan20, to: office.vlan10 }
machines:
  pc1: { networks: { office.vlan10: 10 }, docker: { image: alpine:3.20 } }
  pc2: { networks: { office.vlan20: 10 }, docker: { image: alpine:3.20 } }
  srv: { networks: { servers: 10 }, docker: { image: nginx:1.27-alpine } }
";

    #[test]
    fn vlans_become_networks_and_references_follow() {
        let s = flatten(raw(OFFICE)).unwrap();
        assert_eq!(s.networks.keys().collect::<Vec<_>>(), ["office-vlan10", "office-vlan20", "servers"]);
        assert_eq!(s.networks["office-vlan10"].cidr, "10.10.10.0/24");
        assert!(!s.networks["office-vlan10"].internet, "inherits the LAN's");
        assert!(s.networks["office-vlan20"].internet, "its own");
        assert!(s.machines["pc1"].networks.contains_key("office-vlan10"));
        let rules: Vec<(String, String)> = s.reach.iter().map(|r| (r.from.clone(), r.to.clone())).collect();
        assert_eq!(
            rules,
            [
                ("office-vlan10".into(), "servers".into()),
                ("office-vlan20".into(), "servers".into()),
                ("office-vlan20".into(), "office-vlan10".into())
            ]
        );
        assert!(crate::validate(&s).is_empty(), "{:?}", crate::validate(&s));
    }

    #[test]
    fn the_lan_stays_when_a_machine_joins_it_directly() {
        let yaml = OFFICE
            .replace("cidr: 10.10.0.0/16", "cidr: 10.10.0.0/24")
            .replace("10.10.10.0/24", "10.10.0.64/26")
            .replace("10.10.20.0/24", "10.10.0.128/26")
            + "  printer: { networks: { office: 10 }, docker: { image: alpine:3.20 } }\n";
        let s = flatten(raw(&yaml)).unwrap();
        assert!(s.networks.contains_key("office"));
        assert!(s.reach.iter().any(|r| r.from == "office" && r.to == "servers"));
    }

    #[test]
    fn bad_vlans_are_refused() {
        assert!(
            flatten(raw(&OFFICE.replace("10.10.20.0/24", "10.30.20.0/24")))
                .unwrap_err()
                .contains("isn't inside")
        );
        assert!(
            flatten(raw(&OFFICE.replace("      10:", "      5000:")))
                .unwrap_err()
                .contains("between 1 and 4094")
        );
        let clash = OFFICE.replace(
            "  servers: { cidr: 10.20.0.0/24 }",
            "  servers: { cidr: 10.20.0.0/24 }\n  office-vlan10: { cidr: 10.30.0.0/24 }",
        );
        assert!(flatten(raw(&clash)).unwrap_err().contains("already has"));
    }

    #[test]
    fn the_lans_switch_follows_its_vlans() {
        let yaml = OFFICE.replace("    internet: false\n    vlans:", "    internet: false\n    switch: sw\n    vlans:")
            + "  sw: { docker: { image: x, appliance: cisco-iol-l2 } }\n";
        let s = flatten(raw(&yaml)).unwrap();
        assert_eq!(switch_of(&s, "office"), Some("sw"));
        assert_eq!(switched_by(&s, "sw"), Some("office"));
        assert_eq!(switched_by(&s, "pc1"), None);
        assert_eq!(
            of_lan(&s, "office").collect::<Vec<_>>(),
            [(&"office-vlan10".to_string(), 10), (&"office-vlan20".to_string(), 20)]
        );
        assert!(crate::validate(&s).is_empty(), "{:?}", crate::validate(&s));
    }

    #[test]
    fn a_spec_without_vlans_is_untouched() {
        let s =
            raw("version: 1\nname: x\nnetworks:\n  lan: { cidr: 10.1.0.0/24 }\nmachines:\n  a: { networks: { lan: 10 }, docker: { image: alpine:3.20 } }\n");
        assert_eq!(flatten(s.clone()).unwrap(), s);
    }
}
