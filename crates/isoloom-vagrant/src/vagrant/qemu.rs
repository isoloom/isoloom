//! Each VM's link under Vagrant's QEMU provider. Without root, QEMU's `socket` netdev joins
//! exactly two VMs (one listens, the other connects, on a loopback port of the host), and the
//! plugin gives a VM one private network.

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachineShape};

use super::{CONTROLLER, TOOL_SHELL};
use crate::router;

/// A VM's link on its private network under QEMU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// On no network.
    None,
    /// QEMU's `socket` netdev options: `listen=` for the first VM of the pair, `connect=` for
    /// the second.
    Socket(String),
    /// Why QEMU can't give it its network (the VM then has none there).
    Refused(String),
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
                    if on.len() == 2 {
                        let side = if on[0] == name { "listen" } else { "connect" };
                        Link::Socket(format!("{side}=127.0.0.1:{}", port(&env.name, net)))
                    } else {
                        Link::Refused(format!("network `{net}` has {} VMs, and QEMU links exactly two without root", on.len()))
                    }
                }
                more => Link::Refused(format!("`{name}` is on {} networks, and QEMU gives a VM one", more.len())),
            };
            (name.clone(), link)
        })
        .collect()
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
}
