//! 802.1Q trunks: a machine on several VLANs of one LAN gets one tagged link instead of one
//! interface per VLAN, as a router-on-a-stick or a server on a trunk port has.
//!
//! **Containers** run their trunks through a switch container, which the Compose backend adds
//! (`isoloom-compose`'s trunks): it shares the trunks, the interface names and the way an
//! interface is found here.
//!
//! **VMs.** The hypervisor carries each VLAN on its own network, one NIC each, as for any
//! network; inside, a veth pair stands for the trunk: its end named after the LAN holds the
//! addresses on `<lan>.<id>`, the other end's `.<id>` joins a bridge with the VLAN's NIC. Tagged
//! frames cross the veth (a capture on `<lan>` shows them). Each `<lan>.<id>` takes its NIC's MAC
//! address and the NIC, now only a bridge port, a local one, so the hypervisor still sees only
//! the MAC it gave the VM (no promiscuous mode, on every provider).

use std::net::Ipv4Addr;

use isoloom_plan::Environment;
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

/// The trunks of the machines the plan runs as Linux VMs: each on two or more VLANs of one LAN.
#[must_use]
pub(crate) fn vms(env: &Environment) -> Vec<Trunk> {
    of_machines(env.spec(), |name, _| env.vm(name).is_some_and(|vm| !vm.plan.is_windows()))
}

/// The trunks of the machines `kept` keeps (by name and spec): a trunk per machine and LAN with
/// two VLANs or more.
#[must_use]
pub fn of_machines(spec: &Spec, kept: impl Fn(&str, &Machine) -> bool) -> Vec<Trunk> {
    let mut out: Vec<Trunk> = Vec::new();
    for (name, m) in spec.machines.iter().filter(|(n, m)| kept(n, m)) {
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

/// The interface name on the machine's end: the LAN's (`office`, then `office.10`), within
/// Linux's 15 characters with room for `.<id>`.
#[must_use]
pub fn interface(lan: &str) -> String {
    lan.chars().take(10).collect::<String>().trim_end_matches('-').to_owned()
}

/// Shell that sets `IF` to the interface holding this address.
#[must_use]
pub fn find_by_address(addr: Ipv4Addr) -> String {
    format!(
        "IF=$(ip -o -4 addr show | awk '$4 ~ /^{}\\//{{print $2}}' | head -n 1)",
        addr.to_string().replace('.', "\\.")
    )
}

/// The veth's other end, inside the VM (8 characters of the LAN, `-s`, then `.<id>` fits 15).
fn vm_peer(lan: &str) -> String {
    format!("{}-s", lan.chars().take(8).collect::<String>().trim_end_matches('-'))
}

/// Commands that build a VM's trunk (once per boot: they do nothing when it's there).
#[must_use]
pub(crate) fn vm_commands(env: &Environment, t: &Trunk) -> Vec<String> {
    let name = interface(&t.lan);
    let peer = vm_peer(&t.lan);
    let mut steps = vec![
        format!("ip link add {name} type veth peer name {peer}"),
        format!("ip link set {name} up"),
        format!("ip link set {peer} up"),
    ];
    for v in &t.vlans {
        let (id, addr) = (v.id, env.address_on(&v.network, v.octet));
        let len = env.block(&v.network).prefix();
        // The NIC keeps a local MAC; `<lan>.<id>` takes the one the hypervisor gave it.
        let local = format!("02:1f:{}", addr.octets().map(|o| format!("{o:02x}")).join(":"));
        steps.push(format!(
            "{} && MAC=$(cat \"/sys/class/net/$IF/address\") && ip addr flush dev \"$IF\" && ip link set \"$IF\" address {local}",
            find_by_address(addr)
        ));
        steps.push(format!(
            "ip link add br{id} type bridge && ip link set \"$IF\" master br{id} && ip link set br{id} up"
        ));
        steps.push(format!(
            "ip link add link {peer} name {peer}.{id} type vlan id {id} && ip link set {peer}.{id} master br{id} && ip link set {peer}.{id} up"
        ));
        steps.push(format!(
            "ip link add link {name} name {name}.{id} address \"$MAC\" type vlan id {id} && ip addr add {addr}/{len} dev {name}.{id} && ip link set {name}.{id} up"
        ));
    }
    vec![format!("ip link show {name} >/dev/null 2>&1 || {{ {}; }}", steps.join(" && "))]
}

#[cfg(test)]
mod tests;
