//! The firewall rules a cloud writes per network, read from the plan: who may reach a network
//! (its own machines, the `reach` rules into it), where its machines may go (their network, the
//! `reach` rules out of it, the internet).

use std::net::Ipv4Addr;

use isoloom_plan::Environment;
use isoloom_spec::validate::{Cidr, PRIVATE};

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
