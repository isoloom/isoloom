//! Where each network lands on the Docker target: its block, moved inside 10.0.0.0/8 when it
//! is elsewhere.

use super::Cidr;
use crate::model::Spec;

/// The first octet of the 192.168.0.0/16 private range, the one block whose second octet is
/// fixed (168) and so is mapped by its first on Docker.
const PRIVATE_192_FIRST_OCTET: u8 = 192;
/// Where the scan for a free block starts when the mapped one is taken: 10.240.0.0, the top of
/// 10.0.0.0/8, above anything a spec writes by hand, up to the range's end (10.255.255.255 + 1).
const AUTO_MOVE_FROM: u32 = 0x0af0_0000;
const AUTO_MOVE_END: u32 = 0x0b00_0000;

/// Where the Docker target keeps its networks: 172.16.0.0/12 and 192.168.0.0/16 clash with
/// Docker's own pools, Docker Desktop's network and home LANs.
pub(crate) const DOCKER_BLOCK: Cidr = match Cidr::new(0x0a00_0000, 8) {
    Some(c) => c,
    None => Cidr::ALL,
};

/// Each network's block on the Docker target: its `docker.cidr` when set; its `cidr` when it's
/// inside 10.0.0.0/8 or outside the environment (`external`, `bridged`); otherwise moved there
/// automatically, keeping its size (so each machine keeps its last octet): 192.168.X.0 -> 10.192.X.0, 172.N.X.0 -> 10.N.X.0, or the next free
/// block when that one is taken. Validated specs only: a network whose block doesn't parse is
/// left out (validation reports it).
#[must_use]
pub fn docker_cidrs(spec: &Spec) -> Vec<(String, Cidr)> {
    let mut fixed: Vec<(String, Cidr)> = Vec::new();
    let mut moved: Vec<(String, Cidr)> = Vec::new();
    for (name, n) in &spec.networks {
        let Some(c) = Cidr::parse(&n.cidr) else { continue };
        match n.docker.as_ref().and_then(|d| Cidr::parse(&d.cidr)) {
            Some(d) => fixed.push((name.clone(), d)),
            // A network outside the environment keeps its own block: it exists as it is.
            None if DOCKER_BLOCK.contains(c) || n.is_outside() => fixed.push((name.clone(), c)),
            None => moved.push((name.clone(), c)),
        }
    }
    let mut taken: Vec<Cidr> = fixed.iter().map(|(_, c)| *c).collect();
    let mut out: indexmap::IndexMap<String, Cidr> = fixed.into_iter().collect();
    for (name, c) in moved {
        let pick = moved_block(c, &taken);
        taken.push(pick);
        out.insert(name, pick);
    }
    spec.networks.keys().filter_map(|n| Some((n.clone(), *out.get(n)?))).collect()
}

/// Where a block outside 10.0.0.0/8 moves on Docker: the mapped block, else the next free one
/// of the same size.
fn moved_block(block: Cidr, taken: &[Cidr]) -> Cidr {
    let [first_octet, second, third, fourth] = block.address().octets();
    // 192.168.x.y keeps its 192 as the second octet (10.192.x.y), as a block of 172.16/12 or
    // 10/8 keeps its own second octet: a mapping a reader can invert by eye.
    let first = if first_octet == PRIVATE_192_FIRST_OCTET {
        u32::from_be_bytes([10, PRIVATE_192_FIRST_OCTET, third, fourth])
    } else {
        u32::from_be_bytes([10, second, third, fourth])
    };
    let size = usize::try_from(1u64 << (32 - block.prefix())).unwrap_or(usize::MAX);
    // The mapped block, else the next free one of the same size scanning 10.240.0.0 upward
    // within 10.0.0.0/8: a range stepped by the block's size, so `find` always ends. Running
    // out needs thousands of auto-moved networks: the mapped block is kept then, and
    // validation reports the overlap.
    let scan = (AUTO_MOVE_FROM..AUTO_MOVE_END).step_by(size);
    std::iter::once(first)
        .chain(scan)
        .filter_map(|base| Cidr::new(base, block.prefix()))
        .find(|cand| DOCKER_BLOCK.contains(*cand) && !taken.iter().any(|t| t.overlaps(*cand)))
        .or_else(|| Cidr::new(first, block.prefix()))
        .unwrap_or(block)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(text: &str) -> Cidr {
        Cidr::parse(text).unwrap()
    }

    #[test]
    fn a_moved_block_steps_by_its_own_size_past_the_blocks_taken() {
        let taken = [block("10.16.0.0/28"), block("10.240.0.0/29")];
        assert_eq!(moved_block(block("172.16.0.0/28"), &taken), block("10.240.0.16/28"));
    }

    /// The Docker blocks of the networks `networks` (YAML) describes.
    fn docker(networks: &str) -> Vec<(String, String)> {
        let spec = crate::testing::model(&format!("version: 1\nname: t\nnetworks:\n{networks}")).unwrap();
        docker_cidrs(&spec).into_iter().map(|(n, c)| (n, c.to_string())).collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect()
    }

    #[test]
    fn a_block_outside_10_moves_into_it_keeping_its_octets() {
        assert_eq!(
            docker("  home: { cidr: 192.168.5.0/24 }\n  dmz: { cidr: 172.20.3.0/24 }\n  lab: { cidr: 10.7.0.0/24 }\n"),
            pairs(&[("home", "10.192.5.0/24"), ("dmz", "10.20.3.0/24"), ("lab", "10.7.0.0/24")])
        );
    }

    #[test]
    fn a_networks_own_docker_block_and_an_outside_network_stay_as_written() {
        assert_eq!(
            docker("  home: { cidr: 192.168.5.0/24, docker: { cidr: 10.50.0.0/24 } }\n  corp: { cidr: 192.168.50.0/24, external: corp-net }\n"),
            pairs(&[("home", "10.50.0.0/24"), ("corp", "192.168.50.0/24")])
        );
    }

    #[test]
    fn a_taken_block_moves_to_the_next_free_one_from_10_240() {
        assert_eq!(
            docker("  lab: { cidr: 10.192.5.0/24 }\n  home: { cidr: 192.168.5.0/24 }\n  also: { cidr: 192.168.5.0/24 }\n"),
            pairs(&[("lab", "10.192.5.0/24"), ("home", "10.240.0.0/24"), ("also", "10.240.1.0/24")])
        );
        assert_eq!(
            docker("  lab: { cidr: 10.240.0.0/23 }\n  dmz: { cidr: 172.240.0.0/24 }\n"),
            pairs(&[("lab", "10.240.0.0/23"), ("dmz", "10.240.2.0/24")])
        );
    }
}
