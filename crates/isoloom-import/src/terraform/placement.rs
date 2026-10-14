//! Where the drafted machines sit on the drafted networks, and the YAML written from it.

use std::collections::BTreeSet;

use indexmap::IndexMap;

use super::Machine;
use crate::validate::Cidr;
use crate::yaml::{DraftYaml, Header};
use crate::{cidr_text, network_names};

/// The network the draft adds for machines without an address when nothing else is used.
const FALLBACK_NETWORK: &str = "10.88.1.0/24";

/// Where the machines sit: the networks used, by block, with the subnet's name (empty when
/// none), and each machine's networks with its address's last octet.
pub(super) struct Placement {
    used: IndexMap<String, String>,
    placed: Vec<Vec<(String, u8)>>,
}

impl Placement {
    /// Each machine joins the networks its addresses fall in; addresses outside every subnet
    /// make a /24 network of their own.
    pub(super) fn new(machines: &[Machine], nets: &IndexMap<String, (Cidr, String)>) -> Self {
        let mut used: IndexMap<String, String> = IndexMap::new();
        let mut placed: Vec<Vec<(String, u8)>> = Vec::new();
        for m in machines {
            let mut joins = Vec::new();
            for ip in &m.addresses {
                let ipn = u32::from(*ip);
                let (cidr, named) = nets
                    .values()
                    .find(|(c, _)| c.contains(Cidr { base: ipn, len: 32 }))
                    .cloned()
                    .unwrap_or_else(|| {
                        (
                            Cidr {
                                base: ipn & 0xffff_ff00,
                                len: 24,
                            },
                            String::new(),
                        )
                    });
                let key = cidr_text(cidr);
                used.entry(key.clone()).or_insert(named);
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
            self.used.insert(FALLBACK_NETWORK.into(), "lab".into());
            FALLBACK_NETWORK.into()
        };
        let mut taken: Vec<u8> = self.placed.iter().flatten().filter(|(k, _)| *k == key).map(|(_, o)| *o).collect();
        let mut next = 10u8;
        for joins in self.placed.iter_mut().filter(|j| j.is_empty()) {
            while taken.contains(&next) {
                next += 1;
            }
            joins.push((key.clone(), next));
            taken.push(next);
        }
    }

    pub(super) fn write(&self, machines: &[Machine], name: &str, source: &str) -> String {
        let names = network_names(self.used.iter().map(|(key, named)| (key.clone(), (!named.is_empty()).then(|| named.clone()))));
        let mut y = DraftYaml::new(&Header {
            tool: "terraform",
            source,
            review: "Review it with the notes the import printed.",
            next: "add `vm.provision` (or `provision:`) to configure the machines, and `checks` that prove the behavior.",
            name,
        });
        for (key, n) in &names {
            y.network(n, 0, key, true);
        }
        y.machines();
        let mut taken = BTreeSet::new();
        for (m, joins) in machines.iter().zip(&self.placed) {
            let mut name = m.name.clone();
            while !taken.insert(name.clone()) {
                name.push_str("-2");
            }
            y.machine(&name);
            if !joins.is_empty() {
                y.machine_networks(joins.iter().map(|(k, o)| (&names[k], *o)));
            }
            y.resources(m.cpus, m.memory_mb);
            y.vm_os(m.os);
        }
        y.finish()
    }
}
