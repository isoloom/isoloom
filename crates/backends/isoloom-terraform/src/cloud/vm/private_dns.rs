//! The private DNS zone (`dns.private`) on the clouds that have one: the zone, bound to the
//! environment's network, and one A record per name of the plan's zone (each machine's name and
//! aliases, at its first address), which each cloud writes in its own resources
//! (`crate::cloud::<cloud>::vm::private_dns`). DigitalOcean and Linode have no private DNS: their
//! VMs' hosts files carry the names, as on every VM target.

use isoloom_plan::private_dns::{PrivateDns, Record};
use isoloom_render::hcl_builder::{Expression, list, string};

use crate::module::ident;

/// How long a resolver keeps an answer: short, since addresses only change when a machine is
/// rebuilt.
pub(crate) const TTL: u64 = 60;

/// The zone's records, each with its Terraform name (unique: the full name made a label).
pub(crate) fn records(zone: &PrivateDns) -> impl Iterator<Item = (String, &Record)> {
    zone.records.iter().map(|r| (ident(&r.name), r))
}

/// One address, as a record's list of values.
pub(crate) fn address(r: &Record) -> Expression {
    list([string(r.address.to_string())])
}
