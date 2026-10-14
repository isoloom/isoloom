//! Terraform: for each provider, every resource type it offers and, for what Isoloom writes,
//! every setting, argument and nested block, from `terraform providers schema -json` with the
//! provider pinned (`coverage/terraform/`, refreshed by scripts/refresh-terraform.sh). Every
//! entry is classified by a family rule, reviewed: an entry no rule names is unclassified and
//! fails the coverage tests, so a provider's new resource types and arguments get reviewed
//! instead of silently counted. The tests also check the rows marked as written against every
//! module the generators write for the committed examples.

mod aws;
mod azure;
mod digitalocean;
mod esxi;
mod google;
mod linode;
mod oci;
mod proxmox;
mod support;
pub mod table;
mod utility;

use super::Format;

pub use table::Table;

/// Every Terraform table built from a provider's schema: Proxmox, ESXi, the six clouds, then
/// the utility providers.
pub const TABLES: &[Table] = &[
    proxmox::TABLE,
    esxi::TABLE,
    aws::TABLE,
    azure::TABLE,
    google::TABLE,
    digitalocean::TABLE,
    linode::TABLE,
    oci::TABLE,
    utility::TABLE,
];

/// The Terraform tables, in that order.
pub fn formats() -> Vec<Format> {
    TABLES.iter().map(Table::format).collect()
}
