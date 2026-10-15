//! Each VM's link under Vagrant's QEMU provider. The plugin gives a VM one private network, a
//! QEMU `socket` netdev. Without root, two VMs link as a pair: one listens, the other connects,
//! on a loopback port of the host. Multicast, the plugin's other no-root mode, doesn't send on a
//! Mac (Darwin refuses to send from a socket bound to the group), so a network with any other
//! number of VMs gets a hub instead: one QEMU process with no machine, a `listen` port per VM
//! on one emulated hub, each VM connecting to its own ([`QEMU_RB`]). vmnet, TAP and
//! `socket_vmnet` would need root (or a daemon running as root): Isoloom uses none of them.

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachineShape};
use isoloom_render::router;

use super::{CONTROLLER, TOOL_SHELL};
use crate::stable_hash;

/// The Ruby the QEMU provider's hubs and disk sizes use (`vagrant_qemu.rb`): the
/// `IsoloomQemu` module, in the preamble of a Vagrantfile that needs it ([`needs_ruby`]).
pub(crate) const QEMU_RB: &str = include_str!("../vagrant_qemu.rb");

/// A VM's link on its private network under QEMU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Link {
    /// On no network.
    None,
    /// QEMU's `socket` netdev options: `listen=` for the first VM of the pair, `connect=` for
    /// the second.
    Socket(String),
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
            Link::Socket(opts) => Some(opts.clone()),
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

/// Each VM's link, by its define name.
pub(crate) fn links(env: &Environment, with_controller: bool) -> IndexMap<String, Link> {
    let vms = vms_and_networks(env, with_controller);
    let members = |net: &str| -> Vec<&str> { vms.iter().filter(|(_, nets)| nets.contains(&net)).map(|(n, _)| n.as_str()).collect() };
    vms.iter()
        .map(|(name, nets)| {
            let link = match nets.as_slice() {
                [] => Link::None,
                [net] => {
                    let on = members(net);
                    let at = on.iter().position(|n| n == name).unwrap_or_default();
                    if on.len() == 2 {
                        let side = if at == 0 { "listen" } else { "connect" };
                        Link::Socket(format!("{side}=127.0.0.1:{}", port(&env.name, net)))
                    } else {
                        Link::Hub(HubPort {
                            net: (*net).to_owned(),
                            port: hub_port(&env.name, net, at),
                        })
                    }
                }
                more => Link::Refused(format!("`{name}` is on {} networks, and QEMU gives a VM one", more.len())),
            };
            (name.clone(), link)
        })
        .collect()
}

/// Each network with a hub, and its VMs with their ports, in the order the VMs are defined:
/// the hub listens on every port, and stops once none of the VMs is left.
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

/// The host port a network's QEMU link uses: stable for the environment and network, in
/// 20000-39999.
fn port(env: &str, net: &str) -> u16 {
    let offset = stable_hash::of(&format!("{env}/{net}")) % u32::from(PORTS);
    FIRST_PORT + u16::try_from(offset).unwrap_or_default()
}

/// The hub port of a network's `at`-th VM: the network's port, then one more a VM (wrapping
/// within the range).
fn hub_port(env: &str, net: &str, at: usize) -> u16 {
    let offset = u16::try_from(at % usize::from(PORTS)).unwrap_or_default();
    FIRST_PORT + (port(env, net) - FIRST_PORT + offset) % PORTS
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
    #[test]
    fn ports_are_stable_and_in_range() {
        let p = super::port("lab", "app");
        assert_eq!(p, super::port("lab", "app"));
        assert!((20000..40000).contains(&p));
        assert_ne!(p, super::port("lab", "db"));
    }

    #[test]
    fn hub_ports_follow_the_network_port_and_stay_in_range() {
        let base = super::port("lab", "app");
        assert_eq!(super::hub_port("lab", "app", 0), base);
        for at in [1, 2, 50, 19_999, 20_000] {
            let p = super::hub_port("lab", "app", at);
            assert!((20000..40000).contains(&p), "{at}: {p}");
        }
        assert_ne!(super::hub_port("lab", "app", 1), base);
    }

    #[test]
    fn hubs_name_every_vm_of_the_network_with_its_port() {
        use super::{HubPort, Link};
        let hub = |port| Link::Hub(HubPort { net: "app".into(), port });
        let links: indexmap::IndexMap<String, Link> = [
            ("r".to_owned(), hub(30000)),
            ("lone".to_owned(), Link::None),
            ("a".to_owned(), hub(30001)),
            ("pair".to_owned(), Link::Socket("listen=127.0.0.1:1".into())),
            ("b".to_owned(), hub(30002)),
        ]
        .into_iter()
        .collect();
        let hubs = super::hubs(&links);
        // Every VM is named, not only the first: the hub stops once none of them is left.
        let app: Vec<(&str, u16)> = hubs["app"].iter().map(|(vm, p)| (vm.as_str(), *p)).collect();
        assert_eq!(app, [("r", 30000), ("a", 30001), ("b", 30002)]);
        assert_eq!(hubs.len(), 1);
    }
}
