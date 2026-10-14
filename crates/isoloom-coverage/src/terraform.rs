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
mod google;
mod linode;
mod oci;
mod proxmox;
mod support;
pub mod table;
mod utility;

use super::vagrant::names;
use super::{
    Format,
    Support::{Planned, Tooling, Unclassified},
};

pub use table::Table;

/// Every Terraform table built from a provider's schema: Proxmox, the six clouds, then the
/// utility providers.
pub const TABLES: &[Table] = &[
    proxmox::TABLE,
    aws::TABLE,
    azure::TABLE,
    google::TABLE,
    digitalocean::TABLE,
    linode::TABLE,
    oci::TABLE,
    utility::TABLE,
];

/// The Terraform tables: Proxmox, ESXi, one per cloud, and the utility providers.
pub fn formats() -> Vec<Format> {
    let mut all: Vec<Format> = TABLES.iter().map(Table::format).collect();
    all.insert(1, esxi());
    all
}

const ESXI: &str = include_str!("../coverage/terraform/esxi.txt");

/// Standalone ESXi hosts (no vCenter), with the community provider josenk/esxi: no generator
/// writes it yet, so its resource types are the whole table.
fn esxi() -> Format {
    let rows = names(ESXI)
        .into_iter()
        .map(|r| {
            let s = match r {
                "esxi_guest" | "esxi_portgroup" | "esxi_vswitch" | "esxi_virtual_disk" => Planned {
                    note: "with the ESXi generator: a VM per machine, a port group per network",
                },
                "esxi_resource_pool" => Tooling {
                    note: "where the host places the VMs: no change in behavior",
                },
                _ => Unclassified,
            };
            (format!("resource {r}"), s)
        })
        .collect();
    Format {
        name: "Terraform: ESXi",
        file: ".isoloom/esxi/ (planned)",
        source: "josenk/esxi 1.10.3 (its resource types)",
        rows,
        collapse_not_portable: false,
    }
}
