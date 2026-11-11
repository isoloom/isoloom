//! Each VM's link under Vagrant's QEMU provider. The plugin gives a VM one private network, a
//! QEMU `socket` netdev. Multicast, the plugin's other no-root mode, doesn't send on a Mac
//! (Darwin refuses to send from a socket bound to the group), so every network gets a hub: one
//! QEMU process with no machine, a `listen` port per VM on one emulated hub, each VM connecting
//! to its own ([`QEMU_RB`]). vmnet, TAP and `socket_vmnet` would need root (or a daemon running
//! as root): Isoloom uses none of them.
//!
//! The hub ports are the host's, so they belong to the copy of the project that runs, not to the
//! spec: two copies of one environment (two folders, one name) run side by side. The Vagrantfile
//! picks them when Vagrant reads it (`IsoloomQemu.ports`), free on the host and kept while a hub
//! runs on them; what Isoloom writes is which VM has which network's port.

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachineShape};
use isoloom_render::router;

use super::{CONTROLLER, TOOL_SHELL};
use crate::ruby::Value;

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

/// A VM's port on its network's hub: `QEMU_HUBS[net][vm]`, picked when Vagrant reads the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HubPort {
    /// The network, as `QEMU_HUBS` keys it.
    pub(crate) net: String,
    /// The VM, by its define name, as `QEMU_HUBS[net]` keys it.
    pub(crate) vm: String,
}

impl Link {
    /// The `socket` netdev options the VM runs with, when it has a link: a Ruby string, its port
    /// read from `QEMU_HUBS`.
    pub(crate) fn socket_opts(&self) -> Option<Value> {
        match self {
            Link::Hub(h) => Some(Value::raw(format!(
                "\"connect=127.0.0.1:#{{QEMU_HUBS[{}][{}]}}\"",
                Value::str(&h.net),
                Value::str(&h.vm)
            ))),
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
    if env.tools.contains_key("shell") {
        vms.push((TOOL_SHELL.to_owned(), all.clone()));
    }
    if with_controller {
        vms.push((CONTROLLER.to_owned(), all));
    }
    vms
}

/// Each VM's link, by its define name: a VM on one network gets a port of its hub, one host port
/// per linked VM, at most [`PORTS`] of them.
pub(crate) fn links(env: &Environment, with_controller: bool) -> IndexMap<String, Link> {
    let mut linked = 0;
    vms_and_networks(env, with_controller)
        .into_iter()
        .map(|(name, nets)| {
            let link = match nets.as_slice() {
                [] => Link::None,
                [net] => {
                    linked += 1;
                    if linked <= usize::from(PORTS) {
                        Link::Hub(HubPort {
                            net: (*net).to_owned(),
                            vm: name.clone(),
                        })
                    } else {
                        Link::Refused(format!("`{name}`: QEMU links at most {PORTS} VMs of an environment, a host port each"))
                    }
                }
                more => Link::Refused(format!("`{name}` is on {} networks, and QEMU gives a VM one", more.len())),
            };
            (name, link)
        })
        .collect()
}

/// Each network with a hub, and its VMs, in the order they are defined: the hub listens on a port
/// for each, and stops once none of them is left.
pub(crate) fn hubs(links: &IndexMap<String, Link>) -> IndexMap<String, Vec<String>> {
    let mut out: IndexMap<String, Vec<String>> = IndexMap::new();
    for link in links.values() {
        if let Link::Hub(h) = link {
            out.entry(h.net.clone()).or_default().push(h.vm.clone());
        }
    }
    out
}

/// Whether the Vagrantfile needs [`QEMU_RB`]: a network has a hub, or a VM a disk size.
pub(crate) fn needs_ruby(links: &IndexMap<String, Link>, sized: bool) -> bool {
    sized || links.values().any(|l| matches!(l, Link::Hub(_)))
}

/// How many host ports the hubs pick from (`IsoloomQemu::PORTS`, from 20000).
const PORTS: u16 = 20_000;

/// The first VM that can't have its network, as the reason.
pub(crate) fn refusal(links: IndexMap<String, Link>) -> Option<String> {
    links.into_values().find_map(|l| match l {
        Link::Refused(why) => Some(why),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::{HubPort, Link};

    fn hub(net: &str, vm: &str) -> Link {
        Link::Hub(HubPort {
            net: net.into(),
            vm: vm.into(),
        })
    }

    #[test]
    fn hubs_name_every_vm_of_the_network() {
        let links: indexmap::IndexMap<String, Link> = [
            ("r".to_owned(), hub("app", "r")),
            ("lone".to_owned(), Link::None),
            ("a".to_owned(), hub("app", "a")),
            ("other".to_owned(), Link::Refused("on two networks".into())),
            ("b".to_owned(), hub("app", "b")),
        ]
        .into_iter()
        .collect();
        let hubs = super::hubs(&links);
        // Every VM is named, not only the first: the hub stops once none of them is left.
        assert_eq!(hubs["app"], ["r", "a", "b"]);
        assert_eq!(hubs.len(), 1);
    }

    #[test]
    fn every_network_is_a_hub_even_with_two_vms() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { os: debian@12, networks: { lab: 10 } }\n  b: { os: debian@12, networks: { lab: 11 } }\n",
        );
        let links = super::links(&env, false);
        assert_eq!(links["a"], hub("lab", "a"));
        assert_eq!(links["b"], hub("lab", "b"));
        assert_eq!(super::hubs(&links)["lab"], ["a", "b"]);
    }

    /// The port is the copy's, read when Vagrant reads the file: nothing of the spec's name.
    #[test]
    fn a_vm_connects_to_its_port_as_the_vagrantfile_picked_it() {
        let opts = hub("app", "web").socket_opts().map(|v| v.to_string());
        assert_eq!(opts.as_deref(), Some(r#""connect=127.0.0.1:#{QEMU_HUBS["app"]["web"]}""#));
        assert_eq!(Link::None.socket_opts(), None);
    }

    #[test]
    fn the_ruby_picks_from_as_many_ports_as_the_links_count() {
        assert!(super::QEMU_RB.contains(&format!("PORTS = {}_{:03}\n", super::PORTS / 1000, super::PORTS % 1000)));
    }
}
