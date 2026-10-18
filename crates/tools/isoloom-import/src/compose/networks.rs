//! The draft's networks: Compose's, each with a block the format accepts, and the addresses
//! handed out on them.

use std::collections::BTreeSet;

use indexmap::IndexMap;
use indexmap::map::Entry;
use serde_yaml_ng::Value;

use super::DEFAULT_NETWORK;
use super::text::str_of;
use crate::draft::{LEFT_OUT, NoteKind, Notes};
use crate::naming::{label, unique};
use crate::{Error, Result};
use isoloom_spec::validate::{Cidr, PRIVATE};

/// Where re-addressed networks go: 10.88.<n>.0/24.
const READDRESS_BASE: &str = "10.88";

/// One network of the draft: its name, block, whether it reaches the internet, and the last
/// octets already handed out on it.
#[derive(Debug)]
pub(super) struct Net {
    pub(super) name: String,
    pub(super) cidr: Cidr,
    pub(super) internet: bool,
    taken: BTreeSet<u8>,
}

impl Net {
    /// Whether the address ending in `octet` is on this network and still free.
    fn free(&self, octet: u8) -> bool {
        self.cidr.host(octet).is_some() && !self.taken.contains(&octet)
    }

    /// Takes the address ending in `octet`: the one asked for when it's usable, else the first
    /// free one from .10. The bool says the asked one wasn't usable. None: the network is full.
    pub(super) fn take(&mut self, asked: Option<u8>) -> Option<(u8, bool)> {
        let (octet, moved) = match asked.filter(|o| self.free(*o)) {
            Some(o) => (o, false),
            None => ((10..=250_u8).find(|o| self.free(*o))?, true),
        };
        self.taken.insert(octet);
        Some((octet, moved))
    }
}

/// The networks by their Compose name, in the order they were first met.
#[derive(Debug, Default)]
pub(super) struct Networks {
    pub(super) nets: IndexMap<String, Net>,
    blocks: Blocks,
}

impl Networks {
    /// Adds a network the file declares under `networks:`.
    pub(super) fn declare(&mut self, compose_name: &str, def: &Value, notes: &mut Notes) -> Result<()> {
        let taken = self.names();
        let net = net(&mut self.blocks, &taken, compose_name, Some(def), notes)?;
        self.nets.insert(compose_name.to_owned(), net);
        Ok(())
    }

    /// A network a service joins, added first when the file doesn't declare it.
    pub(super) fn get(&mut self, compose_name: &str, notes: &mut Notes) -> Result<&mut Net> {
        let taken = self.names();
        match self.nets.entry(compose_name.to_owned()) {
            Entry::Occupied(e) => Ok(e.into_mut()),
            Entry::Vacant(e) => Ok(e.insert(net(&mut self.blocks, &taken, compose_name, None, notes)?)),
        }
    }

    /// The names the draft's networks have so far.
    fn names(&self) -> Vec<String> {
        self.nets.values().map(|n| n.name.clone()).collect()
    }
}

/// A network from its Compose definition (none when it isn't declared), named apart from the
/// names `taken`.
fn net(blocks: &mut Blocks, taken: &[String], compose_name: &str, def: Option<&Value>, notes: &mut Notes) -> Result<Net> {
    let at = format!("networks.{compose_name}");
    let cidr = blocks.of(&at, def, notes)?;
    let internal = def.and_then(|d| d.get("internal")).and_then(Value::as_bool).unwrap_or(false);
    for k in def.and_then(Value::as_mapping).into_iter().flat_map(|m| m.keys().filter_map(str_of)) {
        if matches!(k.as_str(), "ipam" | "internal") || k.starts_with("x-") {
            continue;
        }
        notes.add(format!("{at}.{k}"), NoteKind::Changed, LEFT_OUT);
    }
    let base = if compose_name == DEFAULT_NETWORK {
        "lab".to_owned()
    } else {
        label(compose_name)
    };
    Ok(Net {
        name: unique(&base, |n| taken.iter().any(|t| t == n)),
        cidr,
        internet: !internal,
        taken: BTreeSet::new(),
    })
}

/// The blocks networks get: their own subnet when the draft can use it, else the next
/// 10.88.<n>.0/24.
#[derive(Debug)]
struct Blocks {
    /// The next 10.88.<n>.0/24 block to hand out.
    next: u32,
}

impl Default for Blocks {
    fn default() -> Self {
        Blocks { next: 1 }
    }
}

impl Blocks {
    /// The network's block: its subnet when that's a private /24 to /29, else the next
    /// 10.88.<n>.0/24, with a note.
    fn of(&mut self, at: &str, def: Option<&Value>, notes: &mut Notes) -> Result<Cidr> {
        let subnet = def
            .and_then(|d| d.get("ipam"))
            .and_then(|i| i.get("config"))
            .and_then(Value::as_sequence)
            .and_then(|c| c.first())
            .and_then(|c| c.get("subnet"))
            .and_then(str_of);
        let usable = subnet
            .as_deref()
            .and_then(Cidr::parse)
            .filter(|c| c.len >= 24 && c.len <= 29 && PRIVATE.iter().any(|r| r.contains(*c)));
        if let Some(c) = usable {
            return Ok(c);
        }
        let cidr = Cidr::parse(&format!("{READDRESS_BASE}.{}.0/24", self.next)).ok_or_else(|| Error::NoBlockLeft {
            at: at.to_owned(),
            base: READDRESS_BASE,
        })?;
        self.next += 1;
        let why = match &subnet {
            Some(s) => format!("{s} isn't a private /24 to /29; re-addressed to {cidr}"),
            None => format!("no subnet given; addressed as {cidr}"),
        };
        notes.add(format!("{at}.ipam"), NoteKind::Changed, why);
        Ok(cidr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_asked_for_is_given_when_free_else_the_next_from_ten() -> std::result::Result<(), String> {
        let mut nets = Networks::default();
        let mut notes = Notes::default();
        let net = nets.get("front", &mut notes).map_err(|e| e.to_string())?;
        assert_eq!(net.take(Some(20)), Some((20, false)));
        assert_eq!(net.take(Some(20)), Some((10, true)));
        assert_eq!(net.take(Some(1)), Some((11, true)), "the gateway's address isn't a machine's");
        Ok(())
    }

    #[test]
    fn two_compose_networks_named_alike_get_two_names() -> std::result::Result<(), String> {
        let mut nets = Networks::default();
        let mut notes = Notes::default();
        nets.get("a_b", &mut notes).map_err(|e| e.to_string())?;
        let second = nets.get("a-b", &mut notes).map_err(|e| e.to_string())?;
        assert_eq!(second.name, "a-b-2");
        Ok(())
    }

    #[test]
    fn the_blocks_handed_out_run_out_with_an_error() {
        let mut nets = Networks {
            blocks: Blocks { next: 256 },
            ..Networks::default()
        };
        let e = nets.get("late", &mut Notes::default()).err().map(|e| e.to_string());
        assert_eq!(e.as_deref(), Some("networks.late: no 10.88.<n>.0/24 block left to address it"));
    }
}
