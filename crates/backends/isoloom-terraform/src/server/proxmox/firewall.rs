//! The Proxmox firewall: the `reach` rules enforced again on each machine's own devices, outside
//! the guest, besides the router. A machine accepts what comes from its own networks (the
//! router and the controller among them), what a `reach` rule opens into one of them, and its
//! published ports (forwarded by the router, from anywhere); the rest is dropped. Every network
//! is an IP set of the datacenter (`iso<slot>n<i>`: its IPv4 block and its IPv6 one).
//!
//! The rules take effect where the datacenter's firewall is on (Datacenter, Firewall, Options):
//! Isoloom doesn't turn it on, as that changes the server's own rules.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Expression, Module, Template, boolean, reference, string};
use isoloom_render::router;

use crate::module::ident;
use crate::server::slotted;

/// What a machine runs as, and so how its firewall is named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Guest {
    /// A VM (`proxmox_virtual_environment_vm`).
    Vm,
    /// An LXC container (`proxmox_virtual_environment_container`).
    Container,
}

/// An IP set per network.
pub(super) fn ipsets(env: &Environment, m: &mut Module) {
    for (i, (net, plan)) in env.networks.iter().enumerate() {
        let mut b = Block::builder("resource")
            .add_label("proxmox_virtual_environment_firewall_ipset")
            .add_label(ident(net))
            .add_attribute(("name", slotted(&format!("n{i}"))))
            .add_attribute(("comment", string(format!("{}: network {net}", env.name))))
            .add_block(Block::builder("cidr").add_attribute(("name", string(plan.cidr.as_str()))).build());
        if let Some(v6) = &plan.ipv6 {
            b = b.add_block(Block::builder("cidr").add_attribute(("name", string(v6.cidr.as_str()))).build());
        }
        m.commented(&format!("Network `{net}`, for the machines' firewall rules."), b.build());
    }
}

/// A network's IP set, as a rule names it.
fn set(net: &str) -> Expression {
    Template::text("+")
        .with_expr(reference(&format!("proxmox_virtual_environment_firewall_ipset.{}.name", ident(net))))
        .into_expression()
}

/// An accepting rule into the guest.
fn accept(comment: &str, fields: Vec<(&str, Expression)>) -> Block {
    let mut b = Block::builder("rule")
        .add_attribute(("type", string("in")))
        .add_attribute(("action", string("ACCEPT")))
        .add_attribute(("comment", string(comment)));
    for f in fields {
        b = b.add_attribute(f);
    }
    b.build()
}

/// The rules into machine `name`: its networks, the `reach` rules into them, its published ports.
fn rules(env: &Environment, name: &str) -> Vec<Block> {
    let mp = &env.machines[name];
    let mut out: Vec<Block> = mp.addresses.keys().map(|n| accept(&format!("network {n}"), vec![("source", set(n))])).collect();
    let spec = env.spec();
    let into = spec
        .reach
        .iter()
        .filter(|r| mp.addresses.contains_key(&r.to) && router::plain(spec, &r.from) && router::plain(spec, &r.to));
    for r in into {
        let comment = format!("reach {} to {}", r.from, r.to);
        let ends = || vec![("source", set(&r.from)), ("dest", set(&r.to))];
        if r.ports.is_empty() {
            out.push(accept(&comment, ends()));
            continue;
        }
        let ports = r.ports.iter().map(u16::to_string).collect::<Vec<_>>().join(",");
        for proto in ["tcp", "udp"] {
            let mut f = ends();
            f.extend([("proto", string(proto)), ("dport", string(ports.as_str()))]);
            out.push(accept(&comment, f));
        }
        let mut f = ends();
        f.push(("macro", string("Ping")));
        out.push(accept(&comment, f));
    }
    for p in &spec.machines[name].ports {
        if p.publish.is_some() {
            let f = vec![("proto", string(p.protocol.id())), ("dport", string(p.port.to_string()))];
            out.push(accept("published", f));
        }
    }
    out
}

/// Machine `name`'s firewall: on, dropping what its rules don't accept.
pub(super) fn guest(env: &Environment, name: &str, kind: Guest, m: &mut Module) {
    let (resource, key) = match kind {
        Guest::Vm => ("proxmox_virtual_environment_vm", "vm_id"),
        Guest::Container => ("proxmox_virtual_environment_container", "container_id"),
    };
    let id = || reference(&format!("{resource}.{}.vm_id", ident(name)));
    let options = Block::builder("resource")
        .add_label("proxmox_virtual_environment_firewall_options")
        .add_label(ident(name))
        .add_attribute(("node_name", reference("var.node")))
        .add_attribute((key, id()))
        .add_attribute(("enabled", boolean(true)))
        .add_attribute(("input_policy", string("DROP")))
        .add_attribute(("output_policy", string("ACCEPT")))
        .build();
    m.commented(&format!("Machine `{name}`'s firewall: what reaches it, outside the guest."), options);
    let rules = Block::builder("resource")
        .add_label("proxmox_virtual_environment_firewall_rules")
        .add_label(ident(name))
        .add_attribute(("node_name", reference("var.node")))
        .add_attribute((key, id()))
        .add_blocks(rules(env, name))
        .build();
    m.block(rules);
}
