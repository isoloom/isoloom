//! The subnets that become the draft's networks: a private block each, by the address other
//! resources refer to it by.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_spec::validate::{Cidr, PRIVATE};

use super::instance::Instance;
use crate::draft::{NoteKind, Notes};
use crate::naming::label;

/// What a resource is to the draft's networks.
enum Declared {
    /// A subnet, with its block when it is written as a literal.
    Subnet(Option<String>),
    /// Any other resource.
    Other,
}

/// A subnet resource and its block, for the resource types whose subnets become networks.
fn declared(i: &Instance) -> Declared {
    Declared::Subnet(match i.kind.as_str() {
        "aws_subnet" | "oci_core_subnet" => i.str(&["cidr_block"]),
        "azurerm_subnet" => i.str(&["address_prefixes"]),
        "google_compute_subnetwork" => i.str(&["ip_cidr_range"]),
        "digitalocean_vpc" => i.str(&["ip_range"]),
        "linode_vpc_subnet" => i.str(&["ipv4"]),
        _ => return Declared::Other,
    })
}

/// Whether the import reads this resource as a network.
pub(super) fn reads(i: &Instance) -> bool {
    matches!(declared(i), Declared::Subnet(_))
}

/// Networks: subnets with a private block, by address (`aws_subnet.lab`), with their name.
pub(super) fn networks(all: &[Instance], notes: &mut Notes) -> IndexMap<String, (Cidr, String)> {
    let mut nets = IndexMap::new();
    for i in all {
        let Declared::Subnet(block) = declared(i) else {
            continue;
        };
        let at = format!("resource {}", i.address());
        match block.as_deref().and_then(Cidr::parse) {
            Some(c) if PRIVATE.iter().any(|r| r.contains(c)) => {
                // Isoloom networks are /24 to /29: a bigger subnet keeps its first /24.
                let kept = if c.len < 24 { Cidr { base: c.base, len: 24 } } else { c };
                nets.insert(i.address(), (kept, label(&i.name_or(None))));
                if c.len < 24 {
                    let text = format!("{} is bigger than a /24: kept as {}/24", block.unwrap_or_default(), Ipv4Addr::from(kept.base));
                    notes.add(at, NoteKind::Changed, text);
                }
            }
            _ => notes.add(
                at,
                NoteKind::Changed,
                format!("subnet `{}` couldn't be read as a private block", block.unwrap_or_default()),
            ),
        }
    }
    nets
}
