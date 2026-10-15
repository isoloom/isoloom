//! The draft's networks: Compose's, each with a block the format accepts, and the addresses
//! handed out on them.

use std::collections::BTreeSet;

use indexmap::IndexMap;
use indexmap::map::Entry;
use serde_yaml_ng::Value;

use super::super::{ImportError, LEFT_OUT, NoteKind, Notes, label};
use super::DEFAULT_NETWORK;
use super::values::str_of;
use crate::validate::Cidr;

/// Where re-addressed networks go: 10.88.<n>.0/24.
const READDRESS_BASE: &str = "10.88";

pub(super) struct Net {
    pub name: String,
    pub cidr_text: String,
    pub internet: bool,
    cidr: Cidr,
    taken: BTreeSet<u8>,
}

impl Net {
    /// Whether the address ending in `octet` is on this network and still free.
    fn free(&self, octet: u8) -> bool {
        self.cidr.host(octet).is_some() && !self.taken.contains(&octet)
    }

    /// Takes the address ending in `octet`: the one asked for when it's usable, else the first
    /// free one from .10. The bool says the asked one wasn't usable. None: the network is full.
    pub fn take(&mut self, asked: Option<u8>) -> Option<(u8, bool)> {
        let (octet, moved) = match asked.filter(|o| self.free(*o)) {
            Some(o) => (o, false),
            None => ((10..=250u8).find(|o| self.free(*o))?, true),
        };
        self.taken.insert(octet);
        Some((octet, moved))
    }
}

/// The networks by their Compose name, in the order they were first met.
pub(super) struct Networks {
    pub nets: IndexMap<String, Net>,
    /// The next 10.88.<n>.0/24 block to hand out.
    next_block: u32,
}

impl Networks {
    pub fn new() -> Self {
        Networks {
            nets: IndexMap::new(),
            next_block: 1,
        }
    }

    /// Adds a network the file declares under `networks:`.
    pub fn declare(&mut self, compose_name: &str, def: &Value, notes: &mut Notes) -> Result<(), ImportError> {
        let net = make(&mut self.next_block, compose_name, Some(def), notes)?;
        self.nets.insert(compose_name.to_string(), net);
        Ok(())
    }

    /// A network a service joins, added first when the file doesn't declare it.
    pub fn get(&mut self, compose_name: &str, notes: &mut Notes) -> Result<&mut Net, ImportError> {
        Ok(match self.nets.entry(compose_name.to_string()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(make(&mut self.next_block, compose_name, None, notes)?),
        })
    }
}

/// A network from its Compose definition (none when it isn't declared).
fn make(next_block: &mut u32, compose_name: &str, def: Option<&Value>, notes: &mut Notes) -> Result<Net, ImportError> {
    let at = format!("networks.{compose_name}");
    let (cidr, cidr_text) = block(next_block, &at, def, notes)?;
    let internal = def.and_then(|d| d.get("internal")).and_then(Value::as_bool).unwrap_or(false);
    for (k, _) in def.and_then(Value::as_mapping).into_iter().flatten() {
        let k = k.as_str().unwrap_or_default();
        if matches!(k, "ipam" | "internal") || k.starts_with("x-") {
            continue;
        }
        notes.add(format!("{at}.{k}"), NoteKind::Changed, LEFT_OUT);
    }
    let name = if compose_name == DEFAULT_NETWORK {
        "lab".to_string()
    } else {
        label(compose_name)
    };
    Ok(Net {
        name,
        cidr,
        cidr_text,
        internet: !internal,
        taken: BTreeSet::new(),
    })
}

/// The network's block: its subnet when that's a private /24 to /29, else the next
/// 10.88.<n>.0/24, with a note.
fn block(next_block: &mut u32, at: &str, def: Option<&Value>, notes: &mut Notes) -> Result<(Cidr, String), ImportError> {
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
        .filter(|c| c.len >= 24 && c.len <= 29 && crate::validate::PRIVATE.iter().any(|r| r.contains(*c)));
    if let (Some(c), Some(text)) = (usable, &subnet) {
        return Ok((c, text.clone()));
    }
    let text = format!("{READDRESS_BASE}.{next_block}.0/24");
    *next_block += 1;
    let why = match &subnet {
        Some(s) => format!("{s} isn't a private /24 to /29; re-addressed to {text}"),
        None => format!("no subnet given; addressed as {text}"),
    };
    notes.add(format!("{at}.ipam"), NoteKind::Changed, why);
    let cidr = Cidr::parse(&text).ok_or_else(|| ImportError::Addressing {
        at: at.to_string(),
        reason: format!("no {READDRESS_BASE}.<n>.0/24 block left to address it"),
    })?;
    Ok((cidr, text))
}
