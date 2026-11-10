//! Where every machine sits: its address on each network, the address another machine reaches
//! it at, its names, and the order machines start in. Pure functions of a validated spec, shared
//! by the plan and every backend (re-exported by `isoloom-render`).

use std::net::Ipv4Addr;

use isoloom_spec::model::{Machine, Spec};
use isoloom_spec::net::NetworkBlock;
use isoloom_spec::validate::starts_after;

/// A block that stands in for one that doesn't parse: 0.0.0.0/24, whose reserved addresses
/// (gateway, controller, router, tools) all exist, so nothing computed from it overflows.
const UNPARSED: NetworkBlock = NetworkBlock::enclosing(std::net::Ipv4Addr::UNSPECIFIED, 24);

/// A network's block: the one place the plan reads a network's `cidr`; the backends read the
/// plan's ([`crate::NetworkPlan::cidr`], [`crate::Environment::block`],
/// [`crate::Environment::address_on`]), as a test of the command line holds them to. Validation
/// rejects a block that doesn't parse or isn't a /24 to /29, and a network nothing declares, so a validated spec never meets `UNPARSED`; an
/// unvalidated one gets 0.0.0.0 addresses in its output instead of a crash.
#[must_use]
pub fn cidr(spec: &Spec, network: &str) -> NetworkBlock {
    spec.networks.get(network).and_then(|n| NetworkBlock::parse(&n.cidr)).unwrap_or(UNPARSED)
}

impl crate::Environment {
    /// Network `network`'s block on this target: the plan's [`crate::NetworkPlan::cidr`], read
    /// the way [`cidr`] reads the spec's, so a network the plan doesn't hold (which nothing of a
    /// validated spec names) has the same stand-in block.
    #[must_use]
    pub fn block(&self, network: &str) -> NetworkBlock {
        self.networks.get(network).map_or_else(|| cidr(self.spec(), network), |n| n.cidr)
    }

    /// The address octet `octet` has on network `network` on this target: [`Self::block`]'s, with
    /// that last byte (a machine's own are its plan's `addresses`).
    #[must_use]
    pub fn address_on(&self, network: &str, octet: u8) -> Ipv4Addr {
        self.block(network).with_octet(octet)
    }
}

/// A machine's address on a network (validated specs only): the block's last byte set to the
/// machine's octet. Validation keeps the octet a host address of the block or its gateway's,
/// so the address lies in the block.
#[must_use]
pub fn address(spec: &Spec, network: &str, octet: u8) -> Ipv4Addr {
    cidr(spec, network).with_octet(octet)
}

/// The host's own address on a network (Docker's bridge): the gateway address, unless a
/// machine is the network's gateway; then the router's address, unused on such networks.
#[must_use]
pub(crate) fn host_address(spec: &Spec, network: &str) -> Ipv4Addr {
    let block = cidr(spec, network);
    if spec.networks.get(network).is_some_and(|n| n.gateway.is_some()) {
        block.router()
    } else {
        block.gateway()
    }
}

/// The address machine `from` should use to reach machine `to`: on a network they share if
/// any, else `to`'s first address. `0.0.0.0` when `to` has none: it is on no network (or isn't
/// a machine of the spec, which validation refuses wherever a machine is named).
#[must_use]
pub(crate) fn address_for(spec: &Spec, from: &str, to: &str) -> Ipv4Addr {
    let shared = |n: &str| spec.machines.get(from).is_some_and(|a| a.networks.contains_key(n));
    let Some(b) = spec.machines.get(to) else {
        return Ipv4Addr::UNSPECIFIED;
    };
    b.networks
        .iter()
        .find(|(n, _)| shared(n))
        .or_else(|| b.networks.first())
        .map_or(Ipv4Addr::UNSPECIFIED, |(net, octet)| address(spec, net, *octet))
}

/// A machine's names for a hosts line: its own, then its `aliases`.
#[must_use]
pub fn names_of(spec: &Spec, name: &str) -> String {
    std::iter::once(name)
        .chain(spec.machines.get(name).map(|m| m.aliases.iter().map(String::as_str)).into_iter().flatten())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Machines in start order: each after the machines it starts after (its `depends_on`, its
/// networks' gateways, the owners of the volumes it shares). Validation refuses a cycle; an
/// unvalidated one is broken where it closes, so the order is still every machine once.
#[must_use]
pub fn start_order(spec: &Spec) -> Vec<&str> {
    let mut order: Vec<&str> = Vec::new();
    for (name, m) in &spec.machines {
        visit(spec, (name, m), &mut Vec::new(), &mut order);
    }
    order
}

/// Puts `name` in `order` after what it starts after. `path` holds the machines being visited:
/// one met again closes a cycle and is skipped, so the recursion is at most as deep as the spec
/// has machines.
fn visit<'a>(spec: &'a Spec, (name, m): (&'a str, &'a Machine), path: &mut Vec<&'a str>, order: &mut Vec<&'a str>) {
    if order.contains(&name) || path.contains(&name) {
        return;
    }
    path.push(name);
    for dep in starts_after(spec, name, m) {
        if let Some((dep, dm)) = spec.machines.get_key_value(dep) {
            visit(spec, (dep, dm), path, order);
        }
    }
    path.pop();
    order.push(name);
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;
    use crate::testing;

    const SPEC: &str = "version: 1
name: t
networks: { a: { cidr: 10.1.0.0/24 }, b: { cidr: 10.2.0.0/25 }, c: { cidr: 10.3.0.0/24, gateway: fw } }
machines:
  web: { networks: { a: 10, b: 20 }, aliases: [www, site], depends_on: [db], app: { image: nginx } }
  db: { networks: { b: 30 }, app: { image: postgres } }
  fw: { networks: { a: 1, c: 1 }, app: { image: alpine } }
  box: { networks: { c: 40 }, app: { image: alpine } }
  lone: { app: { image: alpine } }
";

    #[test]
    fn an_address_is_the_block_with_the_machines_octet() {
        let spec = testing::spec(SPEC);
        assert_eq!(address(&spec, "a", 10), Ipv4Addr::new(10, 1, 0, 10));
        assert_eq!(address(&spec, "b", 30), Ipv4Addr::new(10, 2, 0, 30));
        assert_eq!(cidr(&spec, "b").netmask(), Ipv4Addr::new(255, 255, 255, 128));
        assert_eq!(cidr(&spec, "nowhere"), UNPARSED);
        let wide = testing::spec(&SPEC.replace("10.2.0.0/25", "10.2.0.0/16"));
        assert_eq!(cidr(&wide, "b"), UNPARSED, "no network's size: validation refuses it");
    }

    #[test]
    fn a_network_routed_by_a_machine_gives_the_host_the_routers_address() {
        let spec = testing::spec(SPEC);
        assert_eq!(host_address(&spec, "a"), Ipv4Addr::new(10, 1, 0, 1));
        assert_eq!(host_address(&spec, "c"), Ipv4Addr::new(10, 3, 0, 254));
    }

    #[test]
    fn a_machine_is_reached_on_a_shared_network_else_at_its_first_address() {
        let spec = testing::spec(SPEC);
        assert_eq!(address_for(&spec, "db", "web"), Ipv4Addr::new(10, 2, 0, 20));
        assert_eq!(address_for(&spec, "fw", "web"), Ipv4Addr::new(10, 1, 0, 10));
        assert_eq!(address_for(&spec, "box", "web"), Ipv4Addr::new(10, 1, 0, 10));
        assert_eq!(address_for(&spec, "web", "lone"), Ipv4Addr::UNSPECIFIED);
        assert_eq!(address_for(&spec, "web", "ghost"), Ipv4Addr::UNSPECIFIED);
    }

    #[test]
    fn names_are_the_machines_own_then_its_aliases() {
        let spec = testing::spec(SPEC);
        assert_eq!(names_of(&spec, "web"), "web www site");
        assert_eq!(names_of(&spec, "db"), "db");
    }

    #[test]
    fn a_machine_starts_after_its_dependencies_and_gateways() {
        let spec = testing::spec(SPEC);
        assert_eq!(start_order(&spec), ["db", "web", "fw", "box", "lone"]);
    }

    #[test]
    fn a_cycle_still_orders_every_machine_once() {
        let mut spec = testing::spec(SPEC);
        if let Some(db) = spec.machines.get_mut("db") {
            db.depends_on = vec!["web".into()];
        }
        assert_eq!(start_order(&spec), ["db", "web", "fw", "box", "lone"]);
    }
}
