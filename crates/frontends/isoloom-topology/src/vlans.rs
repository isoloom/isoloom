//! A switch's VLANs, nested under the LAN named after the switch (Isoloom's `vlans`).

use indexmap::IndexMap;
use isoloom_spec::net::Cidr;

use crate::cables::is_link_network;
use crate::document::{NetworkDoc, ReachDoc, VlanDoc};
use crate::draft::Draft;
use crate::{Error, Lan, Result, Topology};

/// What joins a switch's LAN to the VLAN id in a VLAN's name in the spec.
const VLAN_SEPARATOR: &str = ".vlan";

/// The name a VLAN LAN has in the spec: `<switch>.vlan<id>` (`None` for a plain LAN). The format is
/// Isoloom's, which keeps its reader private (`isoloom-spec`'s `vlans`); the compiler only writes
/// it, and Isoloom reads every spec the compiler hands out.
pub(crate) fn vlan_ref(lan: &Lan) -> Option<String> {
    let (switch, id) = vlan_of(lan)?;
    Some(format!("{switch}{VLAN_SEPARATOR}{id}"))
}

/// The switch and VLAN id of a VLAN LAN.
fn vlan_of(lan: &Lan) -> Option<(&str, u16)> {
    Some((lan.switch.as_deref()?, lan.vlan?))
}

/// Refuses what a switch's VLANs can't say in the spec: a LAN with a switch but no VLAN id or the
/// reverse, a `device` on a plain LAN, a switch named like a plain LAN or like
/// a cable's network (its VLANs nest under a network of the switch's name), a VLAN with a `tc`
/// (Isoloom's VLANs have none), and two LANs on the same VLAN of a switch (the spec has one block
/// per VLAN id).
pub(crate) fn check(topo: &Topology) -> Result<()> {
    for (name, lan) in &topo.networks {
        let (given, missing) = match (&lan.switch, lan.vlan) {
            (Some(_), None) => ("switch", "vlan"),
            (None, Some(_)) => ("vlan", "switch"),
            _ if lan.device.is_some() && vlan_of(lan).is_none() => return Err(Error::DeviceOffSwitch { lan: name.clone() }),
            _ => continue,
        };
        return Err(Error::VlanIncomplete {
            lan: name.clone(),
            given,
            missing,
        });
    }
    if let Some((name, _)) = topo.networks.iter().find(|(_, l)| vlan_of(l).is_some() && l.tc.is_some()) {
        return Err(Error::VlanTc { lan: name.clone() });
    }
    let mut seen: IndexMap<(&str, u16), &String> = IndexMap::new();
    for (name, (switch, vlan)) in topo.networks.iter().filter_map(|(n, l)| Some((n, vlan_of(l)?))) {
        if let Some(first) = seen.insert((switch, vlan), name) {
            return Err(Error::VlanTaken {
                switch: switch.to_owned(),
                vlan,
                first: first.clone(),
                second: name.clone(),
            });
        }
    }
    let plain = |name: &String| topo.networks.get(name).is_some_and(|l| vlan_of(l).is_none());
    for switch in topo.networks.values().filter_map(|l| l.switch.as_ref()) {
        if is_link_network(switch) {
            return Err(Error::ReservedName(switch.clone()));
        }
        if plain(switch) {
            return Err(Error::SwitchNameTaken { switch: switch.clone() });
        }
    }
    Ok(())
}

/// The switches' VLANs, nested under each switch's LAN: the LAN's block is the smallest one
/// holding them all, its `internet` the VLANs' when they agree. Machines and `reach` rules name
/// them `<switch>.vlan<id>`.
pub(crate) fn nest(topo: &Topology, draft: &mut Draft) {
    let renamed: IndexMap<&String, String> = topo.networks.iter().filter_map(|(name, lan)| Some((name, vlan_ref(lan)?))).collect();
    if renamed.is_empty() {
        return;
    }
    draft.networks = nested(topo, std::mem::take(&mut draft.networks));
    for parent in draft.networks.values_mut().filter(|n| !n.vlans.is_empty()) {
        settle_parent(parent);
    }
    rename_refs(draft, &renamed);
}

/// The networks with each VLAN moved under its switch's LAN.
fn nested(topo: &Topology, flat: IndexMap<String, NetworkDoc>) -> IndexMap<String, NetworkDoc> {
    let mut networks: IndexMap<String, NetworkDoc> = IndexMap::new();
    for (name, net) in flat {
        let Some((lan, (switch, id))) = topo.networks.get(&name).and_then(|l| Some((l, vlan_of(l)?))) else {
            networks.insert(name, net);
            continue;
        };
        let internet = net.reaches_internet();
        // The VLAN's own block to start with; settled to one covering all its VLANs once they are in.
        let parent = networks.entry(switch.to_owned()).or_insert_with(|| NetworkDoc::new(net.cidr, internet, None));
        if lan.device.is_some() {
            parent.switch.clone_from(&lan.device);
        }
        parent.vlans.insert(
            id,
            VlanDoc {
                cidr: net.cidr,
                internet: Some(internet),
            },
        );
    }
    networks
}

/// A switch's LAN: the block covering its VLANs, and their `internet` said once when they agree.
fn settle_parent(parent: &mut NetworkDoc) {
    parent.cidr = covering(parent.cidr, parent.vlans.values().map(|v| v.cidr));
    let all: Vec<bool> = parent.vlans.values().filter_map(|v| v.internet).collect();
    let agreed = match all.split_first() {
        Some((first, rest)) if rest.iter().all(|i| i == first) => Some(*first),
        _ => None,
    };
    // When they disagree, the LAN keeps the default (yes): a VLAN says so only when it differs.
    parent.set_internet(agreed.unwrap_or(true));
    for v in parent.vlans.values_mut().filter(|v| agreed.is_some() || v.internet == Some(true)) {
        v.internet = None;
    }
}

/// Machines' networks, `reach` rules and the blocks, with each VLAN LAN under its spec name.
fn rename_refs(draft: &mut Draft, renamed: &IndexMap<&String, String>) {
    let rename = |n: &String| renamed.get(n).cloned().unwrap_or_else(|| n.clone());
    draft.blocks = std::mem::take(&mut draft.blocks).into_iter().map(|(n, b)| (rename(&n), b)).collect();
    for m in draft.machines.values_mut() {
        m.networks = m.networks.iter().map(|(n, o)| (rename(n), *o)).collect();
    }
    for r in &mut draft.reach {
        *r = ReachDoc {
            from: rename(&r.from),
            to: rename(&r.to),
        };
    }
}

/// The smallest IPv4 block holding `first` and every one of `others` (at most a /8).
fn covering(first: Cidr, others: impl Iterator<Item = Cidr>) -> Cidr {
    let blocks: Vec<Cidr> = others.collect();
    let mut len = blocks.iter().map(|b| b.prefix()).fold(first.prefix(), u8::min);
    let around = |prefix: u8| Cidr::enclosing(first.address(), prefix);
    while len > 8 && !blocks.iter().all(|b| around(len).contains(*b)) {
        len -= 1;
    }
    around(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cidr(text: &str) -> Cidr {
        Cidr::parse(text).unwrap()
    }

    #[test]
    fn the_covering_block_holds_every_vlan() {
        let covered = |first: &str, others: &[&str]| covering(cidr(first), others.iter().map(|o| cidr(o))).to_string();
        assert_eq!(covered("10.10.10.0/24", &["10.10.20.0/24"]), "10.10.0.0/19");
        assert_eq!(covered("10.10.10.0/24", &[]), "10.10.10.0/24");
        assert_eq!(covered("10.10.10.0/24", &["11.0.0.0/24"]), "10.0.0.0/8", "at most a /8");
    }

    fn vlan(block: &str, internet: bool) -> VlanDoc {
        VlanDoc {
            cidr: cidr(block),
            internet: Some(internet),
        }
    }

    #[test]
    fn vlans_that_agree_say_their_internet_once_on_their_lan() {
        let mut lan = NetworkDoc::new(cidr("10.10.10.0/24"), true, None);
        lan.vlans.insert(10, vlan("10.10.10.0/24", false));
        lan.vlans.insert(20, vlan("10.10.20.0/24", false));
        settle_parent(&mut lan);
        assert_eq!((lan.cidr.to_string().as_str(), lan.internet), ("10.10.0.0/19", Some(false)));
        assert!(lan.vlans.values().all(|v| v.internet.is_none()));
    }

    #[test]
    fn vlans_that_disagree_keep_the_default_and_the_one_without_says_so() {
        let mut lan = NetworkDoc::new(cidr("10.10.10.0/24"), false, None);
        lan.vlans.insert(10, vlan("10.10.10.0/24", true));
        lan.vlans.insert(20, vlan("10.10.20.0/24", false));
        settle_parent(&mut lan);
        assert_eq!(lan.internet, None, "yes, Isoloom's default");
        assert_eq!(lan.vlans.values().map(|v| v.internet).collect::<Vec<_>>(), [None, Some(false)]);
    }

    #[test]
    fn a_vlan_is_named_under_its_switch_and_a_plain_lan_is_not() {
        let lan = |switch: Option<&str>, vlan: Option<u16>| Lan {
            cidr: "10.10.10.0/24".into(),
            internet: true,
            switch: switch.map(str::to_owned),
            vlan,
            device: None,
            tc: None,
        };
        assert_eq!(vlan_ref(&lan(Some("sw1"), Some(10))).as_deref(), Some("sw1.vlan10"));
        assert_eq!(vlan_ref(&lan(Some("sw1"), None)), None);
        assert_eq!(vlan_ref(&lan(None, Some(10))), None);
    }
}
