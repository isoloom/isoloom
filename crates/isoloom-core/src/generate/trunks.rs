//! 802.1Q trunks on Docker: a machine on several VLANs of one LAN gets one tagged link to the
//! LAN's switch instead of one interface per VLAN, as a router-on-a-stick or a server on a trunk
//! port has.
//!
//! Each VLAN stays a Docker network of its own (machines on one VLAN, the router, the checks and
//! the internet work as for any network). The LAN gets a switch container on each of its VLAN
//! networks (at the controller address, unused on Docker) and on one internal link per trunk
//! machine; inside, a bridge per VLAN joins the VLAN network and the trunks' `.<id>`
//! subinterfaces, so frames cross the trunk tagged. The trunk machine's network sidecar names
//! its end after the LAN and puts its addresses on `<lan>.<id>`.

use std::fmt::Write;

use crate::model::{Machine, Spec};
use crate::validate::Cidr;

/// A machine's trunk on a LAN: the VLAN networks it carries (network, VLAN id, last octet).
pub struct Trunk {
    pub lan: String,
    pub machine: String,
    pub vlans: Vec<(String, u16, u8)>,
}

/// Every trunk: a Docker machine on two or more VLANs of the same LAN.
pub fn trunks(spec: &Spec) -> Vec<Trunk> {
    let mut out: Vec<Trunk> = Vec::new();
    for (name, m) in &spec.machines {
        if m.docker.is_none() {
            continue;
        }
        for (net, octet) in &m.networks {
            let Some((lan, id)) = &spec.networks[net].vlan else { continue };
            match out.iter_mut().find(|t| &t.machine == name && &t.lan == lan) {
                Some(t) => t.vlans.push((net.clone(), *id, *octet)),
                None => out.push(Trunk {
                    lan: lan.clone(),
                    machine: name.clone(),
                    vlans: vec![(net.clone(), *id, *octet)],
                }),
            }
        }
    }
    out.retain(|t| t.vlans.len() >= 2);
    out
}

/// The trunks of one machine.
pub fn of<'a>(trunks: &'a [Trunk], machine: &str) -> impl Iterator<Item = &'a Trunk> {
    trunks.iter().filter(move |t| t.machine == machine)
}

/// The VLAN networks a machine reaches through a trunk rather than an interface of its own.
pub fn carried(trunks: &[Trunk], machine: &str, network: &str) -> bool {
    of(trunks, machine).any(|t| t.vlans.iter().any(|(n, _, _)| n == network))
}

/// The Compose network of a trunk: an internal link between the machine and the LAN's switch.
pub fn link_network(t: &Trunk) -> String {
    format!("isoloom-trunk-{}-{}", t.lan, t.machine)
}

/// The LAN's switch service.
pub fn switch_name(lan: &str) -> String {
    format!("isoloom-switch-{lan}")
}

/// The interface name on the machine's end: the LAN's (`office`, then `office.10`), within
/// Linux's 15 characters with room for `.<id>`.
pub fn interface(lan: &str) -> String {
    lan.chars().take(10).collect::<String>().trim_end_matches('-').to_string()
}

/// A locally administered MAC address for one end of a trunk (`side` 0: the machine, 1: the
/// switch), so each end finds its interface whatever Docker names it.
pub fn mac(t: &Trunk, side: u8) -> String {
    let mut h: u32 = 2166136261;
    for b in format!("{}/{}", t.lan, t.machine).bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(16777619);
    }
    let b = h.to_be_bytes();
    format!("02:1e:{:02x}:{:02x}:{:02x}:{:02x}", b[0], b[1], b[2], (b[3] & 0xfe) | side)
}

/// Shell that sets `IF` to the interface with this MAC address.
fn find_by_mac(mac: &str) -> String {
    format!("IF=$(ip -o link | awk 'tolower($0) ~ /{mac}/ {{print $2}}' | cut -d@ -f1 | tr -d : | head -n 1)")
}

/// Shell that sets `IF` to the interface holding this address.
fn find_by_address(addr: std::net::Ipv4Addr) -> String {
    format!(
        "IF=$(ip -o -4 addr show | awk '$4 ~ /^{}\\//{{print $2}}' | head -n 1)",
        addr.to_string().replace('.', "\\.")
    )
}

/// The switch's address on a VLAN network: the controller's, which Docker leaves free.
pub fn switch_address(spec: &Spec, network: &str) -> std::net::Ipv4Addr {
    Cidr::parse(&spec.networks[network].cidr).expect("validated cidr").controller()
}

/// The switch's start: a bridge per VLAN joining the VLAN network and every trunk's `.<id>`.
pub fn switch_script(spec: &Spec, trunks: &[&Trunk]) -> String {
    let mut ids: Vec<(String, u16)> = Vec::new();
    for t in trunks {
        for (net, id, _) in &t.vlans {
            if !ids.iter().any(|(_, i)| i == id) {
                ids.push((net.clone(), *id));
            }
        }
    }
    let mut s = String::new();
    for (net, id) in &ids {
        let _ = write!(
            s,
            "{} && ip addr flush dev $IF && ip link add br{id} type bridge && ip link set $IF master br{id} && ip link set br{id} up && ",
            find_by_address(switch_address(spec, net))
        );
    }
    for t in trunks {
        let _ = write!(s, "{} && ip addr flush dev $IF && ip link set $IF up && ", find_by_mac(&mac(t, 1)));
        for (_, id, _) in &t.vlans {
            let _ = write!(
                s,
                "ip link add link $IF name $IF.{id} type vlan id {id} && ip link set $IF.{id} master br{id} && ip link set $IF.{id} up && "
            );
        }
    }
    s.push_str("exec sleep infinity");
    s
}

/// What the switch's healthcheck waits for: every bridge up.
pub fn switch_ready(trunks: &[&Trunk]) -> String {
    let mut ids: Vec<u16> = trunks.iter().flat_map(|t| t.vlans.iter().map(|(_, id, _)| *id)).collect();
    ids.sort_unstable();
    ids.dedup();
    ids.iter()
        .map(|id| format!("ip link show br{id} | grep -q 'state UP'"))
        .collect::<Vec<_>>()
        .join(" && ")
}

/// The machine's end, first thing its network sidecar does: the link named after the LAN, and
/// its addresses on `<lan>.<id>`. With no other way out, the default route goes through a VLAN
/// with internet (Docker's gateway on it, reached through the switch).
pub fn machine_commands(
    spec: &Spec,
    m: &Machine,
    t: &Trunk,
    address: impl Fn(&str, u8) -> std::net::Ipv4Addr,
    gateway: impl Fn(&str) -> std::net::Ipv4Addr,
) -> Vec<String> {
    let name = interface(&t.lan);
    let mut cmds = vec![format!(
        "{} && ip addr flush dev $IF && ip link set $IF down && ip link set $IF name {name} && ip link set {name} up",
        find_by_mac(&mac(t, 0))
    )];
    for (net, id, octet) in &t.vlans {
        let len = Cidr::parse(&spec.networks[net].cidr).expect("validated cidr").len;
        cmds.push(format!(
            "ip link add link {name} name {name}.{id} type vlan id {id} && ip addr add {}/{len} dev {name}.{id} && ip link set {name}.{id} up",
            address(net, *octet)
        ));
    }
    let other_way_out = m.networks.keys().any(|n| !t.vlans.iter().any(|(v, _, _)| v == n) && spec.networks[n].internet);
    if !other_way_out && let Some((net, _, _)) = t.vlans.iter().find(|(n, _, _)| spec.networks[n].internet) {
        cmds.push(format!("ip route replace default via {}", gateway(net)));
    }
    cmds
}

/// What the machine's sidecar healthcheck waits for: its addresses on the trunk.
pub fn machine_ready(t: &Trunk) -> String {
    let name = interface(&t.lan);
    t.vlans
        .iter()
        .map(|(_, id, _)| format!("ip link show {name}.{id} | grep -q 'UP'"))
        .collect::<Vec<_>>()
        .join(" && ")
}
