//! Networks outside the environment: an existing network the machines join by name
//! (`external`), or the host's own LAN through a bridged interface (`bridged`). The plan keeps
//! them apart from the environment's own networks: the router, the controller, the checks and
//! every target's own networks never see them, and a machine's place on them is an extra
//! interface after its own networks. A target renders them, or refuses them through its
//! capabilities ([`requirements`]).

use std::fmt;
use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_spec::model::{Bridged, Machine, Spec};

use crate::addressing::address;
use crate::requirements::Requirement;

/// A network outside the environment, as the plan keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutsideNetwork {
    /// Its block, as the author wrote it (the network's real one).
    pub cidr: String,
    /// How the machines get onto it.
    pub join: Join,
}

/// How the machines get onto an outside network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Join {
    /// An existing network, by its name on the target.
    Existing(String),
    /// A bridge to an interface of the host.
    Bridged(Bridged),
}

/// A machine's address on an outside network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutsideAddress {
    /// A fixed one, in the network's block.
    Fixed(Ipv4Addr),
    /// One from the LAN's DHCP server.
    Dhcp,
}

/// The spec's outside networks, in spec order.
#[must_use]
pub fn networks(spec: &Spec) -> IndexMap<String, OutsideNetwork> {
    spec.networks
        .iter()
        .filter_map(|(name, n)| {
            let join = match (&n.external, &n.bridged) {
                (Some(e), _) => Join::Existing(e.clone()),
                (None, Some(b)) => Join::Bridged(b.clone()),
                (None, None) => return None,
            };
            Some((name.clone(), OutsideNetwork { cidr: n.cidr.clone(), join }))
        })
        .collect()
}

/// Machine `m`'s addresses on the outside networks it joins, in its order.
#[must_use]
pub fn addresses(spec: &Spec, m: &Machine) -> IndexMap<String, OutsideAddress> {
    m.networks
        .iter()
        .filter_map(|(net, octet)| {
            let n = spec.networks.get(net).filter(|n| n.is_outside())?;
            let a = if n.is_dhcp() {
                OutsideAddress::Dhcp
            } else {
                OutsideAddress::Fixed(address(spec, net, *octet))
            };
            Some((net.clone(), a))
        })
        .collect()
}

/// The spec without its outside networks, as the rest of the plan and the backends read it:
/// the networks gone, and gone from each machine's list.
#[must_use]
pub fn without(spec: &Spec) -> Spec {
    let mut s = spec.clone();
    s.networks.retain(|_, n| !n.is_outside());
    for m in s.machines.values_mut() {
        m.networks.retain(|net, _| spec.networks.get(net).is_none_or(|n| !n.is_outside()));
    }
    s
}

/// What the outside networks ask of a backend: joining an existing network, bridging a host
/// interface. At most one of each, the first network that asks.
#[must_use]
pub fn requirements(spec: &Spec) -> Vec<Requirement> {
    let mut out = Vec::new();
    if let Some((network, n)) = spec.networks.iter().find(|(_, n)| n.external.is_some()) {
        out.push(Requirement::ExternalNetwork {
            network: network.clone(),
            name: n.external.clone().unwrap_or_default(),
        });
    }
    if let Some((network, _)) = spec.networks.iter().find(|(_, n)| n.external.is_none() && n.bridged.is_some()) {
        out.push(Requirement::BridgedNetwork { network: network.clone() });
    }
    out
}

/// Why a target that makes every network it uses refuses existing network `network` (`name`).
pub(crate) fn no_existing_network(f: &mut fmt::Formatter<'_>, network: &str, name: &str) -> fmt::Result {
    write!(
        f,
        "network `{network}` is an existing network (`external: {name}`), and this target makes every network it uses (`docker`, `hosted` and `proxmox` join existing ones)"
    )
}

/// Why a target that bridges nothing refuses bridged network `network`.
pub(crate) fn no_bridge(f: &mut fmt::Formatter<'_>, network: &str) -> fmt::Result {
    write!(
        f,
        "network `{network}` is bridged to a host interface (`bridged`), which this target has no way to do (`vagrant` and `proxmox` bridge one)"
    )
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;
    use crate::{Feature, testing};

    const SPEC: &str = "
version: 1
name: t
networks:
  lan: { cidr: 10.20.0.0/24 }
  corp: { cidr: 192.168.50.0/24 }
  home: { cidr: 10.0.0.0/24, bridged: { interface: en0, dhcp: true } }
machines:
  a: { networks: { lan: 10, corp: 20, home: 0 }, app: { image: nginx } }
overrides:
  docker: { networks: { corp: { external: corp-net } } }
";

    #[test]
    fn outside_networks_stay_apart_from_the_environment_s_own() {
        let env = testing::planned(&testing::valid(SPEC), Target::Docker);
        assert_eq!(env.networks.keys().collect::<Vec<_>>(), ["lan"]);
        assert_eq!(env.spec().networks.len(), 1);
        assert_eq!(env.outside["corp"].join, Join::Existing("corp-net".into()));
        // The existing network keeps its own block on Docker.
        assert_eq!(env.outside["corp"].cidr, "192.168.50.0/24");
        let a = &env.machines["a"];
        assert_eq!(a.addresses.keys().collect::<Vec<_>>(), ["lan"]);
        assert_eq!(a.outside["corp"], OutsideAddress::Fixed("192.168.50.20".parse().unwrap()));
        assert_eq!(a.outside["home"], OutsideAddress::Dhcp);
        let features: Vec<Feature> = env.requirements.iter().map(Requirement::feature).collect();
        assert!(features.contains(&Feature::ExternalNetworks) && features.contains(&Feature::BridgedNetworks));
    }
}
