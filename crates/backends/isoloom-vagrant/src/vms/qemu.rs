//! Each VM's link under Vagrant's QEMU provider. The plugin gives a VM one private network, a
//! QEMU `socket` netdev. Multicast, the plugin's other no-root mode, doesn't send on a Mac
//! (Darwin refuses to send from a socket bound to the group), so every network gets a hub: one
//! QEMU process with no machine, a `listen` port per VM on one emulated hub, each VM connecting
//! to its own ([`QEMU_RB`]). vmnet, TAP and `socket_vmnet` would need root (or a daemon running
//! as root): Isoloom uses none of them.

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachineShape};
use isoloom_render::router;

use super::{CONTROLLER, TOOL_SHELL};
use isoloom_render::stable_hash;

/// The Ruby the QEMU provider's hubs and disk sizes use (`vagrant_qemu.rb`): the
/// `IsoloomQemu` module, in the preamble of a Vagrantfile that needs it ([`needs_ruby`]).
pub(crate) const QEMU_RB: &str = include_str!("../vagrant_qemu.rb");

/// A VM's link on its private network under QEMU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Link {
    /// On no network.
    None,
    /// A port of its network's hub, which the VM connects to.
    Hub(HubPort),
    /// Why QEMU can't give it its network (the VM then has none there).
    Refused(String),
}

/// A VM's port on its network's hub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HubPort {
    /// The network, as `QEMU_HUBS` keys it.
    pub(crate) net: String,
    /// The host port the VM connects to.
    pub(crate) port: u16,
}

impl Link {
    /// The `socket` netdev options the VM runs with, when it has a link.
    pub(crate) fn socket_opts(&self) -> Option<String> {
        match self {
            Link::Hub(h) => Some(format!("connect=127.0.0.1:{}", h.port)),
            Link::None | Link::Refused(_) => None,
        }
    }
}

/// Every VM of the Vagrantfile in the order it is defined (and started), with its networks:
/// the router, the machines, the tool shell, the controller.
fn vms_and_networks(env: &Environment, with_controller: bool) -> Vec<(String, Vec<&str>)> {
    let all: Vec<&str> = env.networks.keys().map(String::as_str).collect();
    let mut vms = Vec::new();
    if let Some(r) = &env.router {
        vms.push((router::NAME.to_owned(), r.keys().map(String::as_str).collect()));
    }
    let machines = env.start_order.iter().filter_map(|name| Some((name, env.machines.get(name)?)));
    for (name, m) in machines.filter(|(_, m)| m.shape == MachineShape::Vm) {
        vms.push((name.clone(), m.addresses.keys().map(String::as_str).collect()));
    }
    if env.spec().tools.contains_key("shell") {
        vms.push((TOOL_SHELL.to_owned(), all.clone()));
    }
    if with_controller {
        vms.push((CONTROLLER.to_owned(), all));
    }
    vms
}

/// Each VM's link, by its define name: a VM on one network gets the environment's next hub
/// port ([`hub_port`]), so no two of its VMs share one, whatever their networks are called.
pub(crate) fn links(env: &Environment, with_controller: bool) -> IndexMap<String, Link> {
    let mut linked = 0;
    vms_and_networks(env, with_controller)
        .into_iter()
        .map(|(name, nets)| {
            let link = match nets.as_slice() {
                [] => Link::None,
                [net] => {
                    linked += 1;
                    match hub_port(&env.name, linked - 1) {
                        Some(port) => Link::Hub(HubPort { net: (*net).to_owned(), port }),
                        None => Link::Refused(format!("`{name}`: QEMU links at most {PORTS} VMs of an environment, a host port each")),
                    }
                }
                more => Link::Refused(format!("`{name}` is on {} networks, and QEMU gives a VM one", more.len())),
            };
            (name, link)
        })
        .collect()
}

/// Each network with a hub, and its VMs with their ports, in the order the VMs are defined: the
/// hub listens on every port, and stops once none of the VMs is left.
pub(crate) fn hubs(links: &IndexMap<String, Link>) -> IndexMap<String, IndexMap<String, u16>> {
    let mut out: IndexMap<String, IndexMap<String, u16>> = IndexMap::new();
    for (vm, link) in links {
        if let Link::Hub(h) = link {
            out.entry(h.net.clone()).or_default().insert(vm.clone(), h.port);
        }
    }
    out
}

/// Whether the Vagrantfile needs [`QEMU_RB`]: a network has a hub, or a VM a disk size.
pub(crate) fn needs_ruby(links: &IndexMap<String, Link>, sized: bool) -> bool {
    sized || links.values().any(|l| matches!(l, Link::Hub(_)))
}

/// The first host port QEMU links use.
const FIRST_PORT: u16 = 20_000;
/// How many host ports they use, from [`FIRST_PORT`].
const PORTS: u16 = 20_000;

/// The first hub port of environment `env`: stable for it, in 20000-39999.
fn first_port(env: &str) -> u16 {
    let offset = stable_hash::of(env) % u32::from(PORTS);
    FIRST_PORT + u16::try_from(offset).unwrap_or_default()
}

/// The environment's `n`-th hub port (its VMs in the order they are defined, whatever their
/// network): its first port, then one more each, wrapping within the range. One block, so no two
/// are the same; `None` past the range's [`PORTS`] ports.
fn hub_port(env: &str, n: usize) -> Option<u16> {
    let offset = u16::try_from(n).ok().filter(|n| *n < PORTS)?;
    Some(FIRST_PORT + (first_port(env) - FIRST_PORT + offset) % PORTS)
}

/// The first VM that can't have its network, as the reason.
pub(crate) fn refusal(links: IndexMap<String, Link>) -> Option<String> {
    links.into_values().find_map(|l| match l {
        Link::Refused(why) => Some(why),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{HubPort, Link};

    #[test]
    fn the_first_port_is_stable_and_in_range() {
        let p = super::first_port("lab");
        assert_eq!(p, super::first_port("lab"));
        assert!((20000..40000).contains(&p));
        assert_ne!(p, super::first_port("other"));
    }

    #[test]
    fn hub_ports_follow_the_first_port_and_stay_in_range() {
        let base = super::first_port("lab");
        assert_eq!(super::hub_port("lab", 0), Some(base));
        let block: Vec<u16> = (0..20_000).filter_map(|n| super::hub_port("lab", n)).collect();
        assert!(block.iter().all(|p| (20000..40000).contains(p)));
        assert_eq!(
            block.iter().collect::<BTreeSet<_>>().len(),
            20_000,
            "the block wraps within the range, each port once"
        );
        assert_eq!(super::hub_port("lab", 20_000), None, "past the range there is no port left");
    }

    #[test]
    fn hubs_name_every_vm_of_the_network_with_its_port() {
        let base = super::first_port("lab");
        let hub = |at: u16| {
            Link::Hub(HubPort {
                net: "app".into(),
                port: base + at,
            })
        };
        let links: indexmap::IndexMap<String, Link> = [
            ("r".to_owned(), hub(0)),
            ("lone".to_owned(), Link::None),
            ("a".to_owned(), hub(1)),
            ("other".to_owned(), Link::Refused("on two networks".into())),
            ("b".to_owned(), hub(2)),
        ]
        .into_iter()
        .collect();
        let hubs = super::hubs(&links);
        // Every VM is named, not only the first: the hub stops once none of them is left.
        let app: Vec<(&str, u16)> = hubs["app"].iter().map(|(vm, p)| (vm.as_str(), *p)).collect();
        assert_eq!(app, [("r", base), ("a", base + 1), ("b", base + 2)]);
        assert_eq!(hubs.len(), 1);
    }

    #[test]
    fn every_network_is_a_hub_even_with_two_vms() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { os: debian@12, networks: { lab: 10 } }\n  b: { os: debian@12, networks: { lab: 11 } }\n",
        );
        let links = super::links(&env, false);
        let base = super::first_port("t");
        assert_eq!(links["a"], Link::Hub(HubPort { net: "lab".into(), port: base }));
        assert_eq!(
            links["b"],
            Link::Hub(HubPort {
                net: "lab".into(),
                port: base + 1
            })
        );
        assert_eq!(super::hubs(&links)["lab"].len(), 2);
    }

    #[test]
    fn no_two_hub_ports_of_an_environment_are_the_same_whatever_its_networks() {
        // `n9` and `n235` in `lab` hashed to ports one apart when each network had its own
        // base: the first network's second VM took the second network's first port.
        let env = crate::testing::environment(
            "version: 1\nname: lab\nnetworks:\n  n9: { cidr: 10.9.0.0/24 }\n  n235: { cidr: 10.10.0.0/24 }\nmachines:\n  a: { os: debian@12, networks: { n9: 10 } }\n  b: { os: debian@12, networks: { n9: 11 } }\n  c: { os: debian@12, networks: { n235: 10 } }\n  d: { os: debian@12, networks: { n235: 11 } }\n",
        );
        let ports: Vec<u16> = super::links(&env, false)
            .values()
            .filter_map(|l| match l {
                Link::Hub(h) => Some(h.port),
                _ => None,
            })
            .collect();
        assert_eq!(ports.len(), 4);
        assert_eq!(ports.iter().collect::<BTreeSet<_>>().len(), 4, "{ports:?}");
    }
}
