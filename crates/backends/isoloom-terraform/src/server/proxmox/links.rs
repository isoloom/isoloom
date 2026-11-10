//! The environment's links on the server: a VNet per network, a LAN split into VLANs as one
//! VLAN-aware VNet whose VLANs are tags on the devices (802.1Q, as a switch carries them), and
//! each device's MTU (`networks.*.mtu`), rate limit (`machines.*.rate_limit`) and firewall.
//!
//! A machine on several VLANs of one LAN gets an access device per VLAN, each tagged: the
//! addresses and the routing are the same as over a trunk, and the guest needs no VLAN set-up.

use std::collections::BTreeMap;

use crate::hcl_builder::{Block, BlockBuilder, Expression, Module, boolean, number, reference};
use isoloom_plan::{Environment, MachinePlan};

use crate::module::ident;
use crate::server::slot_name;

/// The VNet a network's frames travel on: its own, or its LAN's when it is a VLAN (then with
/// its tag).
fn segment(env: &Environment, net: &str) -> (String, Option<u16>) {
    match env.networks.get(net).and_then(|p| p.vlan.as_ref()) {
        Some((lan, id)) => (lan.clone(), Some(*id)),
        None => (net.to_owned(), None),
    }
}

/// Whether network or LAN `name` carries VLANs.
fn carries_vlans(env: &Environment, name: &str) -> bool {
    env.networks.values().any(|n| n.vlan.as_ref().is_some_and(|(l, _)| l == name))
}

/// Each VNet the environment's links travel on, with its id after `i<slot>`, in the order they
/// are made: one per network that is no VLAN (`n<i>`, `i` its place among every network), then
/// one per LAN that is only the frame around its VLANs (`l<j>`). The module gives each VNet
/// its id, and the run manifest names each network's bridge after them.
pub(super) fn vnet_ids(env: &Environment) -> Vec<(&str, String)> {
    let mut out: Vec<(&str, String)> = env
        .networks
        .iter()
        .enumerate()
        .filter(|(_, (_, p))| p.vlan.is_none())
        .map(|(i, (net, _))| (net.as_str(), format!("n{i}")))
        .collect();
    let mut lans: Vec<&str> = env.networks.values().filter_map(|n| n.vlan.as_ref().map(|(l, _)| l.as_str())).collect();
    lans.dedup();
    let frames = lans.into_iter().filter(|l| !env.networks.contains_key(*l));
    out.extend(frames.enumerate().map(|(j, lan)| (lan, format!("l{j}"))));
    out
}

/// What starts a VNet's id, before the slot: Proxmox takes 8 characters at most.
pub(super) const VNET: &str = "i";

/// The id ([`vnet_ids`]) of the VNet each network's devices are bridged to ([`on`]): its own, or
/// its LAN's for a VLAN.
pub(super) fn vnet_of(env: &Environment) -> BTreeMap<&str, String> {
    let ids: BTreeMap<&str, String> = vnet_ids(env).into_iter().collect();
    env.networks
        .keys()
        .filter_map(|net| Some((net.as_str(), ids.get(segment(env, net).0.as_str())?.clone())))
        .collect()
}

/// A VNet per network that is no VLAN, then one per LAN that is only the frame around its
/// VLANs ([`vnet_ids`]). Their references, for the applier to wait on.
pub(super) fn vnets(env: &Environment, m: &mut Module) -> Vec<Expression> {
    let mut out = Vec::new();
    for (name, id) in vnet_ids(env) {
        let mut b = Block::builder("resource")
            .add_label("proxmox_sdn_vnet")
            .add_label(ident(name))
            .add_attribute(("id", slot_name(VNET, &id).hcl()))
            .add_attribute(("zone", reference("proxmox_sdn_zone_simple.env.id")));
        if carries_vlans(env, name) {
            b = b.add_attribute(("vlan_aware", boolean(true)));
        }
        let what = match env.networks.get(name) {
            Some(plan) => format!("Network `{name}`: {}", plan.cidr),
            None => format!("LAN `{name}`: its VLANs are tags on one VNet"),
        };
        m.commented(&what, b.build());
        out.push(reference(&format!("proxmox_sdn_vnet.{}", ident(name))));
    }
    out
}

/// The zone's MTU: the largest a network asks for, so every VNet's bridge carries its own.
pub(super) fn zone_mtu(env: &Environment) -> Option<u16> {
    env.networks.values().filter_map(|n| n.mtu).max()
}

/// The settings of a device on network `net`: its bridge, its VLAN tag and its MTU, then a
/// machine's rate limit and firewall. `b` is a `network_device` or `network_interface` block.
pub(super) fn on(env: &Environment, net: &str, b: BlockBuilder, guest: Option<&MachinePlan>) -> BlockBuilder {
    let (vnet, tag) = segment(env, net);
    let mut b = b.add_attribute(("bridge", reference(&format!("proxmox_sdn_vnet.{}.id", ident(&vnet)))));
    if let Some(tag) = tag {
        b = b.add_attribute(("vlan_id", number(tag.into())));
    }
    if let Some(mtu) = env.networks.get(net).and_then(|p| p.mtu) {
        b = b.add_attribute(("mtu", number(mtu.into())));
    }
    match guest {
        Some(mp) => limited(b, mp).add_attribute(("firewall", boolean(true))),
        None => b,
    }
}

/// The machine's rate limit on a device, in MB/s as Proxmox takes it.
pub(super) fn limited(b: BlockBuilder, mp: &MachinePlan) -> BlockBuilder {
    let rate = mp.rate_limit.as_deref().and_then(isoloom_plan::links::megabytes_per_second);
    match rate {
        Some(mbs) => b.add_attribute(("rate_limit", Expression::from(mbs))),
        None => b,
    }
}

/// A VM's device on network `net` (see [`on`]), with a fixed MAC address when given.
pub(super) fn device(env: &Environment, net: &str, mac: Option<Expression>, guest: Option<&MachinePlan>) -> Block {
    let mut b = on(env, net, Block::builder("network_device"), guest);
    if let Some(mac) = mac {
        b = b.add_attribute(("mac_address", mac));
    }
    b.build()
}
