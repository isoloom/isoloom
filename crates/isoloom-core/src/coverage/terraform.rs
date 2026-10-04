//! Terraform: each provider's resource types, from `terraform providers schema -json`
//! (`coverage/terraform/*.txt`, refreshed by scripts/terraform-resources.sh). Every type must
//! be classified here, or the coverage tests fail. No Terraform generator exists yet: the
//! resources an environment needs are "not yet"; the arguments of each are detailed when its
//! generator is built.

use super::vagrant::names;
use super::{Format, Support, Support::*};

const PROXMOX: &str = include_str!("../../coverage/terraform/proxmox.txt");
const LEGACY: &str = "proxmox_virtual_environment_";

const GENERATOR: Support = Planned {
    note: "with the Proxmox generator",
};
const SERVER_ADMIN: Support = ByDesign {
    why: "administers the Proxmox server itself, not an environment on it",
};
const HOST_NETWORK: Support = ByDesign {
    why: "changes the server's own network; an environment gets its own SDN networks",
};
const MULTI_NODE: Support = Open {
    note: "multi-node clusters (a simple zone covers one node)",
};
const LXC: Support = Open {
    note: "LXC containers: a way to run `docker:` machines on Proxmox",
};
const FIREWALL: Support = Open {
    note: "the Proxmox firewall: reach rules enforced outside the VMs, besides the router",
};

pub fn formats() -> Vec<Format> {
    let all = names(PROXMOX);
    let rows = all
        .iter()
        .map(|r| {
            // Older names of resources that also exist under the short name.
            let current = r.strip_prefix(LEGACY).map(|s| format!("proxmox_{s}"));
            let s = match current {
                Some(c) if all.contains(&c.as_str()) => Tooling {
                    note: "older name of the resource without `virtual_environment_`",
                },
                _ => proxmox(r.strip_prefix(LEGACY).or_else(|| r.strip_prefix("proxmox_")).unwrap_or(r)).unwrap_or(Unclassified),
            };
            (format!("resource {r}"), s)
        })
        .collect();
    vec![Format {
        name: "Terraform: Proxmox",
        file: ".isoloom/proxmox/ (planned)",
        source: "bpg/proxmox 0.115.0 (its resource types)",
        rows,
    }]
}

fn proxmox(r: &str) -> Option<Support> {
    Some(match r {
        "vm" | "download_file" | "file" | "sdn_zone_simple" | "sdn_vnet" | "sdn_applier" | "pool" | "pool_membership" => GENERATOR,
        "cloned_vm" => Planned {
            note: "linked clones: faster starts from one image (no change in behavior)",
        },
        "sdn_subnet" => ByDesign {
            why: "a subnet's gateway lives on the host bridge, which would route between networks; the router VM holds it",
        },
        "vm2" => Tooling {
            note: "an older, experimental VM resource",
        },
        "container" | "oci_image" => LXC,
        "firewall_rules" | "firewall_options" | "firewall_ipset" | "firewall_alias" => FIREWALL,
        r if r.starts_with("sdn_zone_") || r.starts_with("sdn_fabric") || r.starts_with("sdn_controller") => MULTI_NODE,
        r if r.starts_with("network_") => HOST_NETWORK,
        "node_firewall" | "cluster_firewall" | "cluster_firewall_security_group" => SERVER_ADMIN,
        r if [
            "acl",
            "acme_",
            "apt_",
            "backup_job",
            "ceph_",
            "cluster_options",
            "ha",
            "hardware_mapping_",
            "metrics_server",
            "node_",
            "realm_",
            "replication",
            "storage_",
            "user",
            "certificate",
            "dns",
            "hosts",
            "group",
            "role",
            "time",
        ]
        .iter()
        .any(|p| r.starts_with(p)) =>
        {
            SERVER_ADMIN
        }
        _ => return None,
    })
}
