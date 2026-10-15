//! 802.1Q trunks: a machine on several VLANs of one LAN gets one tagged link instead of one
//! interface per VLAN, as a router-on-a-stick or a server on a trunk port has.
//!
//! **Containers.** Each VLAN stays a Docker network of its own (machines on one VLAN, the router,
//! the checks and the internet work as for any network). The LAN gets a switch container on each
//! of its VLAN networks (at the controller address, unused on Docker) and on one internal link
//! per trunk machine; inside, a bridge per VLAN joins the VLAN network and the trunks' `.<id>`
//! subinterfaces, so frames cross the trunk tagged. The trunk machine's network sidecar names
//! its end after the LAN and puts its addresses on `<lan>.<id>`. A LAN with a `switch` of its
//! own (a switch appliance) gets no switch container: the appliance takes its place on the same
//! networks, and the trunk machines' end is the same.
//!
//! **VMs.** The hypervisor carries each VLAN on its own network, one NIC each, as for any
//! network; inside, a veth pair stands for the trunk: its end named after the LAN holds the
//! addresses on `<lan>.<id>`, the other end's `.<id>` joins a bridge with the VLAN's NIC. Tagged
//! frames cross the veth (a capture on `<lan>` shows them). Each `<lan>.<id>` takes its NIC's MAC
//! address and the NIC, now only a bridge port, a local one, so the hypervisor still sees only
//! the MAC it gave the VM (no promiscuous mode, on every provider).

use std::net::Ipv4Addr;

use isoloom_plan::addressing::cidr;
use isoloom_spec::images::is_windows;
use isoloom_spec::model::{Machine, Spec};

/// One VLAN a trunk carries.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Vlan {
    /// The VLAN's network.
    pub network: String,
    /// Its 802.1Q id.
    pub id: u16,
    /// The machine's last octet on it.
    pub octet: u8,
}

/// A machine's trunk on a LAN: the VLANs it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Trunk {
    /// The LAN the VLANs are tagged on.
    pub lan: String,
    /// The machine carrying the trunk.
    pub machine: String,
    /// Each VLAN it carries, in the machine's order.
    pub vlans: Vec<Vlan>,
}

/// The trunks of the machines run as containers: each on two or more VLANs of one LAN.
#[must_use]
pub fn containers(spec: &Spec) -> Vec<Trunk> {
    of_machines(spec, |m| m.docker.is_some())
}

/// The trunks of the machines run as Linux VMs: each on two or more VLANs of one LAN.
#[must_use]
pub fn vms(spec: &Spec) -> Vec<Trunk> {
    of_machines(spec, |m| m.vm.as_ref().is_some_and(|vm| !is_windows(&vm.os)))
}

/// The trunks of the machines `kept` keeps: a trunk per machine and LAN with two VLANs or more.
fn of_machines(spec: &Spec, kept: impl Fn(&Machine) -> bool) -> Vec<Trunk> {
    let mut out: Vec<Trunk> = Vec::new();
    for (name, m) in spec.machines.iter().filter(|(_, m)| kept(m)) {
        for (net, octet) in &m.networks {
            let Some((lan, id)) = spec.networks.get(net).and_then(|n| n.vlan.as_ref()) else {
                continue;
            };
            let vlan = Vlan {
                network: net.clone(),
                id: *id,
                octet: *octet,
            };
            match out.iter_mut().find(|t| &t.machine == name && &t.lan == lan) {
                Some(t) => t.vlans.push(vlan),
                None => out.push(Trunk {
                    lan: lan.clone(),
                    machine: name.clone(),
                    vlans: vec![vlan],
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

/// Whether a machine reaches a VLAN network through a trunk rather than an interface of its own.
#[must_use]
pub fn carried(trunks: &[Trunk], machine: &str, network: &str) -> bool {
    of(trunks, machine).any(|t| t.vlans.iter().any(|v| v.network == network))
}

/// The Compose network of a trunk: an internal link between the machine and the LAN's switch.
#[must_use]
pub fn link_network(t: &Trunk) -> String {
    format!("isoloom-trunk-{}-{}", t.lan, t.machine)
}

/// The LAN's switch service.
#[must_use]
pub fn switch_name(lan: &str) -> String {
    format!("isoloom-switch-{lan}")
}

/// The interface name on the machine's end: the LAN's (`office`, then `office.10`), within
/// Linux's 15 characters with room for `.<id>`.
#[must_use]
pub fn interface(lan: &str) -> String {
    lan.chars().take(10).collect::<String>().trim_end_matches('-').to_owned()
}

/// One end of a trunk's link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// The machine's end.
    Machine,
    /// The switch's end.
    Switch,
}

/// A locally administered MAC address for one end of a trunk, so each end finds its interface
/// whatever Docker names it: FNV-1a of `<lan>/<machine>`, the last bit telling the ends apart.
#[must_use]
pub fn mac(t: &Trunk, end: End) -> String {
    let hash = format!("{}/{}", t.lan, t.machine)
        .bytes()
        .fold(2_166_136_261_u32, |hash, byte| (hash ^ u32::from(byte)).wrapping_mul(16_777_619));
    let [first, second, third, last] = hash.to_be_bytes();
    let side = match end {
        End::Machine => 0,
        End::Switch => 1,
    };
    format!("02:1e:{first:02x}:{second:02x}:{third:02x}:{:02x}", (last & 0xfe) | side)
}

/// Shell that sets `IF` to the interface with this MAC address.
fn find_by_mac(mac: &str) -> String {
    format!("IF=$(ip -o link | awk 'tolower($0) ~ /{mac}/ {{print $2}}' | cut -d@ -f1 | tr -d : | head -n 1)")
}

/// Shell that sets `IF` to the interface holding this address.
fn find_by_address(addr: Ipv4Addr) -> String {
    format!(
        "IF=$(ip -o -4 addr show | awk '$4 ~ /^{}\\//{{print $2}}' | head -n 1)",
        addr.to_string().replace('.', "\\.")
    )
}

/// The switch's address on a VLAN network: the controller's, which Docker leaves free.
#[must_use]
pub fn switch_address(spec: &Spec, network: &str) -> Ipv4Addr {
    cidr(spec, network).controller()
}

/// The switch's start: a bridge per VLAN joining the VLAN network and every trunk's `.<id>`.
#[must_use]
pub fn switch_script(spec: &Spec, trunks: &[&Trunk]) -> String {
    let mut bridged: Vec<&Vlan> = Vec::new();
    for v in trunks.iter().flat_map(|t| &t.vlans) {
        if !bridged.iter().any(|b| b.id == v.id) {
            bridged.push(v);
        }
    }
    let bridge_lines = bridged.iter().map(|v| {
        format!(
            "{} && ip addr flush dev $IF && ip link add br{id} type bridge && ip link set $IF master br{id} && ip link set br{id} up && ",
            find_by_address(switch_address(spec, &v.network)),
            id = v.id
        )
    });
    let trunk_lines = trunks.iter().map(|t| {
        let tagged: String = t
            .vlans
            .iter()
            .map(|v| {
                format!(
                    "ip link add link $IF name $IF.{id} type vlan id {id} && ip link set $IF.{id} master br{id} && ip link set $IF.{id} up && ",
                    id = v.id
                )
            })
            .collect::<Vec<_>>()
            .concat();
        format!(
            "{} && ip addr flush dev $IF && ip link set $IF up && {tagged}",
            find_by_mac(&mac(t, End::Switch))
        )
    });
    bridge_lines
        .chain(trunk_lines)
        .chain(std::iter::once("exec sleep infinity".to_owned()))
        .collect()
}

/// What the switch's healthcheck waits for: every bridge up.
#[must_use]
pub fn switch_ready(trunks: &[&Trunk]) -> String {
    let mut ids: Vec<u16> = trunks.iter().flat_map(|t| t.vlans.iter().map(|v| v.id)).collect();
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
#[must_use]
pub fn machine_commands(spec: &Spec, m: &Machine, t: &Trunk, address: impl Fn(&str, u8) -> Ipv4Addr, gateway: impl Fn(&str) -> Ipv4Addr) -> Vec<String> {
    let name = interface(&t.lan);
    let mut cmds = vec![format!(
        "{} && ip addr flush dev $IF && ip link set $IF down && ip link set $IF name {name} && ip link set {name} up",
        find_by_mac(&mac(t, End::Machine))
    )];
    for v in &t.vlans {
        cmds.push(format!(
            "ip link add link {name} name {name}.{id} type vlan id {id} && ip addr add {}/{} dev {name}.{id} && ip link set {name}.{id} up",
            address(&v.network, v.octet),
            cidr(spec, &v.network).len,
            id = v.id
        ));
    }
    let internet = |n: &str| spec.networks.get(n).is_some_and(|net| net.internet);
    let other_way_out = m.networks.keys().any(|n| !t.vlans.iter().any(|v| &v.network == n) && internet(n));
    if !other_way_out && let Some(v) = t.vlans.iter().find(|v| internet(&v.network)) {
        cmds.push(format!("ip route replace default via {}", gateway(&v.network)));
    }
    cmds
}

/// What the machine's sidecar healthcheck waits for: its addresses on the trunk.
#[must_use]
pub fn machine_ready(t: &Trunk) -> String {
    let name = interface(&t.lan);
    t.vlans
        .iter()
        .map(|v| format!("ip link show {name}.{} | grep -q 'UP'", v.id))
        .collect::<Vec<_>>()
        .join(" && ")
}

/// The veth's other end, inside the VM (8 characters of the LAN, `-s`, then `.<id>` fits 15).
fn vm_peer(lan: &str) -> String {
    format!("{}-s", lan.chars().take(8).collect::<String>().trim_end_matches('-'))
}

/// Commands that build a VM's trunk (once per boot: they do nothing when it's there).
#[must_use]
pub fn vm_commands(spec: &Spec, t: &Trunk, address: impl Fn(&str, u8) -> Ipv4Addr) -> Vec<String> {
    let name = interface(&t.lan);
    let peer = vm_peer(&t.lan);
    let mut steps = vec![
        format!("ip link add {name} type veth peer name {peer}"),
        format!("ip link set {name} up"),
        format!("ip link set {peer} up"),
    ];
    for v in &t.vlans {
        let (id, addr) = (v.id, address(&v.network, v.octet));
        let len = cidr(spec, &v.network).len;
        // The NIC keeps a local MAC; `<lan>.<id>` takes the one the hypervisor gave it.
        let local = format!("02:1f:{}", addr.octets().map(|o| format!("{o:02x}")).join(":"));
        steps.push(format!(
            "{} && MAC=$(cat /sys/class/net/$IF/address) && ip addr flush dev $IF && ip link set $IF address {local}",
            find_by_address(addr)
        ));
        steps.push(format!(
            "ip link add br{id} type bridge && ip link set $IF master br{id} && ip link set br{id} up"
        ));
        steps.push(format!(
            "ip link add link {peer} name {peer}.{id} type vlan id {id} && ip link set {peer}.{id} master br{id} && ip link set {peer}.{id} up"
        ));
        steps.push(format!(
            "ip link add link {name} name {name}.{id} address $MAC type vlan id {id} && ip addr add {addr}/{len} dev {name}.{id} && ip link set {name}.{id} up"
        ));
    }
    vec![format!("ip link show {name} >/dev/null 2>&1 || {{ {}; }}", steps.join(" && "))]
}

#[cfg(test)]
mod tests;
