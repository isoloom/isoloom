//! The containers' 802.1Q trunks ([`isoloom_render::trunks`] has what a VM's trunk shares with
//! them). Each VLAN stays a Docker network of its own (machines on one VLAN, the router, the
//! checks and the internet work as for any network). The LAN gets a switch container on each of
//! its VLAN networks (at the controller address, unused on Docker) and on one internal link per
//! trunk machine; inside, a bridge per VLAN joins the VLAN network and the trunks' `.<id>`
//! subinterfaces, so frames cross the trunk tagged. The trunk machine's network sidecar names its
//! end after the LAN and puts its addresses on `<lan>.<id>`. A LAN with a `switch` of its own (a
//! switch appliance) gets no switch container: the appliance takes its place on the same
//! networks, and the trunk machines' end is the same.

use std::net::Ipv4Addr;

use isoloom_plan::Environment;
pub(crate) use isoloom_render::trunks::Trunk;
use isoloom_render::trunks::{self, Vlan};
use isoloom_spec::model::{Machine, Spec};

/// The trunks of the machines run as containers: each on two or more VLANs of one LAN.
#[must_use]
pub(crate) fn containers(spec: &Spec) -> Vec<Trunk> {
    trunks::of_machines(spec, |_, m| m.docker.is_some())
}

/// The trunks of one machine.
pub(crate) fn of<'a>(trunks: &'a [Trunk], machine: &str) -> impl Iterator<Item = &'a Trunk> {
    trunks.iter().filter(move |t| t.machine == machine)
}

/// Whether a machine reaches a VLAN network through a trunk rather than an interface of its own.
#[must_use]
pub(crate) fn carried(trunks: &[Trunk], machine: &str, network: &str) -> bool {
    of(trunks, machine).any(|t| t.vlans.iter().any(|v| v.network == network))
}

/// The Compose network of a trunk: an internal link between the machine and the LAN's switch.
#[must_use]
pub(crate) fn link_network(t: &Trunk) -> String {
    format!("isoloom-trunk-{}-{}", t.lan, t.machine)
}

/// The LAN's switch service.
#[must_use]
pub(crate) fn switch_name(lan: &str) -> String {
    format!("isoloom-switch-{lan}")
}

/// One end of a trunk's link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    /// The machine's end.
    Machine,
    /// The switch's end.
    Switch,
}

/// A locally administered MAC address for one end of a trunk, so each end finds its interface
/// whatever Docker names it: FNV-1a of `<lan>/<machine>`, the last bit telling the ends apart.
#[must_use]
pub(crate) fn mac(t: &Trunk, end: End) -> String {
    let [first, second, third, last] = isoloom_render::stable_hash::of(&format!("{}/{}", t.lan, t.machine)).to_be_bytes();
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

/// The switch's address on a VLAN network: the controller's, which Docker leaves free.
#[must_use]
pub(crate) fn switch_address(env: &Environment, network: &str) -> Ipv4Addr {
    env.block(network).controller()
}

/// The switch's start: a bridge per VLAN joining the VLAN network and every trunk's `.<id>`.
#[must_use]
pub(crate) fn switch_script(env: &Environment, trunks: &[&Trunk]) -> String {
    let mut bridged: Vec<&Vlan> = Vec::new();
    for v in trunks.iter().flat_map(|t| &t.vlans) {
        if !bridged.iter().any(|b| b.id == v.id) {
            bridged.push(v);
        }
    }
    let bridge_lines = bridged.iter().map(|v| {
        format!(
            "{} && ip addr flush dev $IF && ip link add br{id} type bridge && ip link set $IF master br{id} && ip link set br{id} up && ",
            trunks::find_by_address(switch_address(env, &v.network)),
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
pub(crate) fn switch_ready(trunks: &[&Trunk]) -> String {
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
pub(crate) fn machine_commands(env: &Environment, m: &Machine, t: &Trunk, gateway: impl Fn(&str) -> Ipv4Addr) -> Vec<String> {
    let spec = env.spec();
    let name = trunks::interface(&t.lan);
    let mut cmds = vec![format!(
        "{} && ip addr flush dev $IF && ip link set $IF down && ip link set $IF name {name} && ip link set {name} up",
        find_by_mac(&mac(t, End::Machine))
    )];
    for v in &t.vlans {
        cmds.push(format!(
            "ip link add link {name} name {name}.{id} type vlan id {id} && ip addr add {}/{} dev {name}.{id} && ip link set {name}.{id} up",
            env.address_on(&v.network, v.octet),
            env.block(&v.network).prefix(),
            id = v.id
        ));
    }
    let internet = |n: &str| spec.networks.get(n).is_some_and(|net| net.internet);
    let other_way_out = m.networks.keys().any(|n| !t.vlans.iter().any(|v| &v.network == n) && internet(n));
    if !other_way_out && let Some(v) = t.vlans.iter().find(|v| internet(&v.network)) {
        cmds.push(isoloom_render::route_line("default", gateway(&v.network)));
    }
    cmds
}

/// What the machine's sidecar healthcheck waits for: its addresses on the trunk.
#[must_use]
pub(crate) fn machine_ready(t: &Trunk) -> String {
    let name = trunks::interface(&t.lan);
    t.vlans
        .iter()
        .map(|v| format!("ip link show {name}.{} | grep -q 'UP'", v.id))
        .collect::<Vec<_>>()
        .join(" && ")
}

#[cfg(test)]
mod tests;
