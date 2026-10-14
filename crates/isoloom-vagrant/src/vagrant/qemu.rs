//! Each VM's link under Vagrant's QEMU provider. The plugin gives a VM one private network, a
//! QEMU `socket` netdev. Without root, two VMs link as a pair: one listens, the other connects,
//! on a loopback port of the host. Multicast, the plugin's other no-root mode, doesn't send on a
//! Mac (Darwin refuses to send from a socket bound to the group), so a network with any other
//! number of VMs gets a hub instead: one QEMU process with no machine, a `listen` port per VM
//! on one emulated hub, each VM connecting to its own ([`QEMU_RB`]). vmnet, TAP and
//! socket_vmnet would need root (or a daemon running as root): Isoloom uses none of them.

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachineShape};

use super::{CONTROLLER, TOOL_SHELL};
use crate::router;

/// The Ruby the QEMU provider's hubs and disk sizes use (`vagrant_qemu.rb`): the
/// `IsoloomQemu` module, in the preamble of a Vagrantfile that needs it ([`needs_ruby`]).
pub const QEMU_RB: &str = include_str!("../vagrant_qemu.rb");

/// A VM's link on its private network under QEMU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
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
pub struct HubPort {
    /// The network, as `QEMU_HUBS` keys it.
    pub net: String,
    /// The host port the VM connects to.
    pub port: u16,
    /// Whether the VM is the network's first (destroyed last, by `vagrant destroy`): the hub
    /// stops with it.
    pub first: bool,
}

impl Link {
    /// The `socket` netdev options the VM runs with, when it has a link.
    pub fn socket_opts(&self) -> Option<String> {
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
        vms.push((router::NAME.to_string(), r.keys().map(String::as_str).collect()));
    }
    for name in &env.start_order {
        let m = &env.machines[name];
        if m.shape == MachineShape::Vm {
            vms.push((name.clone(), m.addresses.keys().map(String::as_str).collect()));
        }
    }
    if env.spec().tools.contains_key("shell") {
        vms.push((TOOL_SHELL.to_string(), all.clone()));
    }
    if with_controller {
        vms.push((CONTROLLER.to_string(), all));
    }
    vms
}

/// Each VM's link, by its define name.
pub fn links(env: &Environment, with_controller: bool) -> IndexMap<String, Link> {
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
                            net: (*net).to_string(),
                            port: hub_port(&env.name, net, at),
                            first: at == 0,
                        })
                    }
                }
                more => Link::Refused(format!("`{name}` is on {} networks, and QEMU gives a VM one", more.len())),
            };
            (name.clone(), link)
        })
        .collect()
}

/// Each network with a hub, and its ports (one a VM, in the order the VMs are defined).
pub fn hubs(links: &IndexMap<String, Link>) -> IndexMap<String, Vec<u16>> {
    let mut out: IndexMap<String, Vec<u16>> = IndexMap::new();
    for link in links.values() {
        if let Link::Hub(h) = link {
            out.entry(h.net.clone()).or_default().push(h.port);
        }
    }
    out
}

/// Whether the Vagrantfile needs [`QEMU_RB`]: a network has a hub, or a VM a disk size.
pub fn needs_ruby(links: &IndexMap<String, Link>, sized: bool) -> bool {
    sized || links.values().any(|l| matches!(l, Link::Hub(_)))
}

/// The host port a network's QEMU link uses: stable for the environment and network, in
/// 20000-39999.
fn port(env: &str, net: &str) -> u16 {
    // FNV-1a: stable across builds and platforms (unlike std's hasher).
    let mut h: u32 = 0x811c_9dc5;
    for b in format!("{env}/{net}").bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    20000 + (h % 20000) as u16
}

/// The hub port of a network's `at`-th VM: the network's port, then one more a VM (wrapping
/// within the range).
fn hub_port(env: &str, net: &str, at: usize) -> u16 {
    let offset = u16::try_from(at % 20000).unwrap_or_default();
    20000 + (port(env, net) - 20000 + offset) % 20000
}

/// The first VM that can't have its network, as the reason.
pub fn refusal(links: IndexMap<String, Link>) -> Option<String> {
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
}
