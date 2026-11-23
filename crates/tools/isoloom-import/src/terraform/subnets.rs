//! The subnets that become the draft's networks: a private block each, by the address other
//! resources refer to it by.

use indexmap::IndexMap;
use isoloom_spec::net::{Cidr, NetworkBlock, PRIVATE};

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

/// Networks: subnets with a private block, by address (`aws_subnet.lab`), with their name. A
/// network is a /24 to /29 ([`NetworkBlock`], as Compose's import reads one): a bigger subnet
/// keeps its first /24 and a smaller one the /29 holding it, each with a note.
pub(super) fn networks(all: &[Instance], notes: &mut Notes) -> IndexMap<String, (NetworkBlock, String)> {
    let mut nets = IndexMap::new();
    for i in all {
        let Declared::Subnet(block) = declared(i) else {
            continue;
        };
        let at = format!("resource {}", i.address());
        match block.as_deref().and_then(Cidr::parse) {
            Some(c) if PRIVATE.iter().any(|r| r.contains(c)) => {
                let kept = NetworkBlock::enclosing(c.address(), c.prefix());
                nets.insert(i.address(), (kept, label(&i.name_or(None))));
                if kept.cidr() != c {
                    let size = if c.prefix() < kept.prefix() {
                        "bigger than a /24"
                    } else {
                        "smaller than a /29"
                    };
                    notes.add(at, NoteKind::Changed, format!("{} is {size}: kept as {kept}", block.unwrap_or_default()));
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
