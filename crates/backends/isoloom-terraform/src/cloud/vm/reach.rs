//! The firewall rules a cloud writes, read from the plan: per network, who may reach it (its
//! own machines, the `reach` rules into it) and where its machines may go (their network, the
//! `reach` rules out of it, the internet); per machine, its own networks and the `reach` rules
//! opening them.

use std::net::Ipv4Addr;

use isoloom_plan::{Environment, NetworkPlan};
use isoloom_spec::model::{Machine, Reach};
use isoloom_spec::validate::{Cidr, PRIVATE};

/// Machine `m`'s own networks, each with its plan.
pub(crate) fn own<'a>(env: &'a Environment, m: &'a Machine) -> impl Iterator<Item = (&'a String, &'a NetworkPlan)> {
    m.networks.keys().filter_map(|n| env.networks.get_key_value(n))
}

/// The `reach` rules opening one of machine `m`'s networks to a network it isn't on, each with
/// the plan of the network they open from.
pub(crate) fn opening<'a>(env: &'a Environment, m: &'a Machine) -> impl Iterator<Item = (&'a Reach, &'a NetworkPlan)> {
    env.spec()
        .reach
        .iter()
        .filter(|r| m.networks.contains_key(&r.to) && !m.networks.contains_key(&r.from))
        .filter_map(|r| Some((r, env.networks.get(&r.from)?)))
}

/// A rule between two networks: the other network's block, and its ports (empty: every port).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rule<'a> {
    /// The other network.
    pub(crate) network: &'a str,
    /// Its block.
    pub(crate) cidr: &'a str,
    /// The ports, TCP and UDP (empty: everything).
    pub(crate) ports: &'a [u16],
}

/// The `reach` rules into `net` from the other networks.
pub(crate) fn into<'a>(env: &'a Environment, net: &str) -> Vec<Rule<'a>> {
    let spec = env.spec();
    spec.reach
        .iter()
        .filter(|r| r.to == net && r.from != net)
        .filter_map(|r| {
            let (name, p) = env.networks.get_key_value(&r.from)?;
            Some(Rule {
                network: name,
                cidr: &p.cidr,
                ports: &r.ports,
            })
        })
        .collect()
}

/// The `reach` rules out of `net` to the other networks.
pub(crate) fn out_of<'a>(env: &'a Environment, net: &str) -> Vec<Rule<'a>> {
    let spec = env.spec();
    spec.reach
        .iter()
        .filter(|r| r.from == net && r.to != net)
        .filter_map(|r| {
            let (name, p) = env.networks.get_key_value(&r.to)?;
            Some(Rule {
                network: name,
                cidr: &p.cidr,
                ports: &r.ports,
            })
        })
        .collect()
}

/// The internet as blocks: every IPv4 address outside the private ranges, where the
/// environment's networks are. A cloud whose rules only allow (no deny) lets a machine out to
/// these without opening the environment's other networks.
pub(crate) fn public_blocks() -> Vec<String> {
    let mut out = Vec::new();
    split(Cidr { base: 0, len: 0 }, &mut out);
    out.into_iter().map(|c| format!("{}/{}", Ipv4Addr::from(c.base), c.len)).collect()
}

fn split(block: Cidr, out: &mut Vec<Cidr>) {
    if PRIVATE.iter().any(|p| p.contains(block)) {
        return;
    }
    if !PRIVATE.iter().any(|p| block.overlaps(*p)) {
        out.push(block);
        return;
    }
    let len = block.len + 1;
    split(Cidr { base: block.base, len }, out);
    split(
        Cidr {
            base: block.base | (1u32 << (32 - len)),
            len,
        },
        out,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_public_blocks_leave_out_exactly_the_private_ranges() {
        let blocks: Vec<Cidr> = public_blocks().iter().filter_map(|b| Cidr::parse(b)).collect();
        let size = |c: &Cidr| 1u64 << (32 - c.len);
        let private: u64 = PRIVATE.iter().map(size).sum();
        assert_eq!(blocks.iter().map(size).sum::<u64>() + private, 1u64 << 32);
        assert!(blocks.iter().all(|b| PRIVATE.iter().all(|p| !b.overlaps(*p))));
        assert!(blocks.len() < 40, "{}", blocks.len());
    }
}
