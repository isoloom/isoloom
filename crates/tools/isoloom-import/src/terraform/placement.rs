//! Where the drafted machines sit on the drafted networks, and the YAML written from it.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_spec::validate::Cidr;

use super::vms::Machine;
use crate::draft::writer::{Header, Writer};
use crate::naming::{network_names, unique};

/// The network the draft adds for machines without an address when nothing else is used.
const FALLBACK_NETWORK: Cidr = Cidr { base: 0x0a58_0100, len: 24 };

/// Where the machines sit: the networks used, by block, with the subnet's name (empty when
/// none), and each machine's networks with its address's last octet.
#[derive(Debug)]
pub(super) struct Placement {
    used: IndexMap<String, (Cidr, String)>,
    placed: Vec<Vec<(String, u8)>>,
}

impl Placement {
    /// Each machine joins the networks its addresses fall in; addresses outside every subnet
    /// make a /24 network of their own.
    pub(super) fn new(machines: &[Machine], nets: &IndexMap<String, (Cidr, String)>) -> Self {
        let mut used: IndexMap<String, (Cidr, String)> = IndexMap::new();
        let mut placed: Vec<Vec<(String, u8)>> = Vec::new();
        for m in machines {
            let mut joins = Vec::new();
            for ip in &m.addresses {
                let (cidr, named) = nets
                    .values()
                    .find(|(c, _)| c.contains(host(*ip)))
                    .cloned()
                    .unwrap_or_else(|| (own_block(*ip), String::new()));
                let key = cidr.to_string();
                used.entry(key.clone()).or_insert((cidr, named));
                joins.push((key, ip.octets()[3]));
            }
            placed.push(joins);
        }
        let mut placement = Placement { used, placed };
        placement.place_unaddressed();
        placement
    }

    /// Machines without an address: on the first network (or a `lab` one), at the next free octet.
    fn place_unaddressed(&mut self) {
        if !self.placed.iter().any(Vec::is_empty) {
            return;
        }
        let key = if let Some(k) = self.used.keys().next() {
            k.clone()
        } else {
            let key = FALLBACK_NETWORK.to_string();
            self.used.insert(key.clone(), (FALLBACK_NETWORK, "lab".into()));
            key
        };
        let mut taken: Vec<u8> = self.placed.iter().flatten().filter(|(k, _)| *k == key).map(|(_, o)| *o).collect();
        let mut next = 10_u8;
        for joins in self.placed.iter_mut().filter(|j| j.is_empty()) {
            while taken.contains(&next) {
                next += 1;
            }
            joins.push((key.clone(), next));
            taken.push(next);
        }
    }

    /// The draft: the header, the networks used, then each machine where it sits.
    pub(super) fn write(&self, machines: &[Machine], name: &str, source: &str) -> String {
        let names = network_names(
            self.used
                .iter()
                .map(|(key, (_, named))| (key.clone(), (!named.is_empty()).then(|| named.clone()))),
        );
        let mut y = Writer::new(&Header {
            tool: "terraform",
            source,
            review: "Review it with the notes the import printed.",
            next: "add `app: { steps: [...] }` (or `provision:`) to configure the machines, and `checks` that prove the behavior.",
            name,
        });
        for (key, (cidr, _)) in &self.used {
            y.network(names.get(key).map_or("lab", String::as_str), 0, *cidr, true);
        }
        y.machines();
        let mut taken: Vec<String> = Vec::new();
        for (m, joins) in machines.iter().zip(&self.placed) {
            let machine = unique(&m.name, |n| taken.iter().any(|t| t == n));
            y.machine(&machine);
            taken.push(machine);
            if !joins.is_empty() {
                y.machine_networks(joins.iter().filter_map(|(k, o)| Some((names.get(k)?, *o))));
            }
            y.resources(m.cpus, m.memory_mb);
            y.os(m.os);
        }
        y.finish()
    }
}

/// The one-address block of `ip`.
fn host(ip: Ipv4Addr) -> Cidr {
    Cidr { base: u32::from(ip), len: 32 }
}

/// The /24 an address outside every subnet makes its own network of.
fn own_block(ip: Ipv4Addr) -> Cidr {
    Cidr {
        base: u32::from(ip) & 0xffff_ff00,
        len: 24,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(name: &str, addresses: &[Ipv4Addr]) -> Machine {
        Machine {
            name: name.to_owned(),
            os: "debian@12",
            addresses: addresses.to_vec(),
            cpus: None,
            memory_mb: None,
        }
    }

    #[test]
    fn a_machine_without_an_address_joins_the_first_network_at_the_next_free_octet() {
        let p = Placement::new(&[machine("a", &[Ipv4Addr::new(10, 1, 2, 10)]), machine("b", &[])], &IndexMap::new());
        assert_eq!(p.placed, [vec![("10.1.2.0/24".to_owned(), 10)], vec![("10.1.2.0/24".to_owned(), 11)]]);
    }

    #[test]
    fn with_no_address_at_all_the_machines_get_a_lab_network() {
        let p = Placement::new(&[machine("a", &[])], &IndexMap::new());
        assert_eq!(p.placed, [vec![("10.88.1.0/24".to_owned(), 10)]]);
    }

    #[test]
    fn two_machines_named_alike_are_numbered() {
        let machines = [machine("vm", &[]), machine("vm", &[])];
        let yaml = Placement::new(&machines, &IndexMap::new()).write(&machines, "lab", "main.tf");
        assert!(yaml.contains("\n  vm:\n") && yaml.contains("\n  vm-2:\n"), "{yaml}");
    }
}
