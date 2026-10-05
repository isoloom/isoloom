//! Terraform: each provider's resource types, from `terraform providers schema -json`
//! (`coverage/terraform/*.txt`, refreshed by scripts/terraform-resources.sh). Every type must
//! be classified here, or the coverage tests fail. No Terraform generator exists yet: the
//! resources an environment needs are "not yet"; the arguments of each are detailed when its
//! generator is built.

use super::vagrant::names;
use super::{Format, Support, Support::*};

const PROXMOX: &str = include_str!("../../coverage/terraform/proxmox.txt");
const AWS: &str = include_str!("../../coverage/terraform/aws.txt");
const AZURE: &str = include_str!("../../coverage/terraform/azure.txt");
const GOOGLE: &str = include_str!("../../coverage/terraform/google.txt");
const DIGITALOCEAN: &str = include_str!("../../coverage/terraform/digitalocean.txt");
const LINODE: &str = include_str!("../../coverage/terraform/linode.txt");
const OCI: &str = include_str!("../../coverage/terraform/oci.txt");
const ESXI: &str = include_str!("../../coverage/terraform/esxi.txt");

/// A cloud provider: the resource types an environment needs (one VM per machine on its own
/// networks), and its Windows VM types. Every other type is one of that cloud's managed
/// services, which no other VM target has.
pub struct Cloud {
    pub name: &'static str,
    pub source: &'static str,
    pub list: &'static str,
    pub environment: &'static [&'static str],
    pub windows: &'static [&'static str],
    /// What the cloud generators (cloud-docker, and on AWS cloud-vm) write today.
    pub generated: &'static [&'static str],
}

pub const CLOUDS: &[Cloud] = &[
    Cloud {
        name: "Terraform: AWS",
        source: "hashicorp/aws 6.67.0 (its resource types)",
        list: AWS,
        environment: &[
            "aws_instance",
            "aws_vpc",
            "aws_subnet",
            "aws_internet_gateway",
            "aws_nat_gateway",
            "aws_eip",
            "aws_eip_association",
            "aws_route_table",
            "aws_route_table_association",
            "aws_vpc_ipv4_cidr_block_association",
            "aws_route",
            "aws_security_group",
            "aws_vpc_security_group_ingress_rule",
            "aws_vpc_security_group_egress_rule",
            "aws_network_interface",
            "aws_network_interface_attachment",
            "aws_key_pair",
            "aws_ebs_volume",
            "aws_volume_attachment",
        ],
        windows: &[],
        generated: &[
            "aws_vpc",
            "aws_vpc_ipv4_cidr_block_association",
            "aws_subnet",
            "aws_internet_gateway",
            "aws_route_table",
            "aws_route_table_association",
            "aws_security_group",
            "aws_instance",
            "aws_key_pair",
        ],
    },
    Cloud {
        name: "Terraform: Azure",
        source: "hashicorp/azurerm 5.8.0 (its resource types)",
        list: AZURE,
        environment: &[
            "azurerm_resource_group",
            "azurerm_linux_virtual_machine",
            "azurerm_virtual_network",
            "azurerm_subnet",
            "azurerm_network_interface",
            "azurerm_network_interface_security_group_association",
            "azurerm_network_security_group",
            "azurerm_network_security_rule",
            "azurerm_subnet_network_security_group_association",
            "azurerm_public_ip",
            "azurerm_nat_gateway",
            "azurerm_subnet_nat_gateway_association",
            "azurerm_route_table",
            "azurerm_route",
            "azurerm_subnet_route_table_association",
            "azurerm_managed_disk",
            "azurerm_virtual_machine_data_disk_attachment",
        ],
        windows: &["azurerm_windows_virtual_machine"],
        generated: &[
            "azurerm_resource_group",
            "azurerm_virtual_network",
            "azurerm_subnet",
            "azurerm_public_ip",
            "azurerm_network_security_group",
            "azurerm_network_interface",
            "azurerm_network_interface_security_group_association",
            "azurerm_linux_virtual_machine",
        ],
    },
    Cloud {
        name: "Terraform: Google Cloud",
        source: "hashicorp/google 8.5.0 (its resource types)",
        list: GOOGLE,
        environment: &[
            "google_compute_instance",
            "google_compute_network",
            "google_compute_subnetwork",
            "google_compute_firewall",
            "google_compute_address",
            "google_compute_route",
            "google_compute_router",
            "google_compute_router_nat",
            "google_compute_disk",
            "google_compute_attached_disk",
        ],
        windows: &[],
        generated: &[
            "google_compute_network",
            "google_compute_subnetwork",
            "google_compute_firewall",
            "google_compute_instance",
        ],
    },
    Cloud {
        name: "Terraform: DigitalOcean",
        source: "digitalocean/digitalocean 2.103.0 (its resource types)",
        list: DIGITALOCEAN,
        environment: &[
            "digitalocean_droplet",
            "digitalocean_vpc",
            "digitalocean_firewall",
            "digitalocean_ssh_key",
            "digitalocean_reserved_ip",
            "digitalocean_reserved_ip_assignment",
            "digitalocean_volume",
            "digitalocean_volume_attachment",
        ],
        windows: &[],
        generated: &["digitalocean_vpc", "digitalocean_ssh_key", "digitalocean_droplet", "digitalocean_firewall"],
    },
    Cloud {
        name: "Terraform: Linode",
        source: "linode/linode 4.7.0 (its resource types)",
        list: LINODE,
        environment: &[
            "linode_instance",
            "linode_instance_config",
            "linode_instance_disk",
            "linode_instance_ip",
            "linode_vpc",
            "linode_vpc_subnet",
            "linode_firewall",
            "linode_sshkey",
            "linode_volume",
        ],
        windows: &[],
        generated: &["linode_instance", "linode_firewall"],
    },
    Cloud {
        name: "Terraform: Oracle Cloud",
        source: "oracle/oci 9.8.0 (its resource types)",
        list: OCI,
        environment: &[
            "oci_core_instance",
            "oci_core_vcn",
            "oci_core_subnet",
            "oci_core_internet_gateway",
            "oci_core_nat_gateway",
            "oci_core_route_table",
            "oci_core_security_list",
            "oci_core_network_security_group",
            "oci_core_network_security_group_security_rule",
            "oci_core_public_ip",
            "oci_core_vnic_attachment",
            "oci_core_volume",
            "oci_core_volume_attachment",
        ],
        windows: &[],
        generated: &[
            "oci_core_vcn",
            "oci_core_internet_gateway",
            "oci_core_route_table",
            "oci_core_security_list",
            "oci_core_subnet",
            "oci_core_instance",
        ],
    },
];

fn cloud(c: &Cloud) -> Format {
    let managed = NotPortable {
        why: "one of this cloud's managed services: no other VM target has it",
    };
    let rows = names(c.list)
        .into_iter()
        .map(|r| {
            let s = if c.generated.contains(&r) {
                Emitted {
                    from: "the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine)",
                }
            } else if c.environment.contains(&r) {
                Planned {
                    note: "with the cloud generator (one VM per machine)",
                }
            } else if c.windows.contains(&r) {
                Planned {
                    note: "with Windows guests (which make an environment VM-only)",
                }
            } else {
                managed
            };
            (format!("resource {r}"), s)
        })
        .collect();
    Format {
        name: c.name,
        file: ".isoloom/cloud/ (planned)",
        source: c.source,
        rows,
        collapse_not_portable: true,
    }
}
const LEGACY: &str = "proxmox_virtual_environment_";

const GENERATOR: Support = Planned {
    note: "with the Proxmox generator",
};
const SERVER_ADMIN: Support = NotPortable {
    why: "administers the Proxmox server itself: no other target has a server to administer",
};
const HOST_NETWORK: Support = NotPortable {
    why: "changes the server's own network: no other target has one to change",
};
const MULTI_NODE: Support = Tooling {
    note: "how a cluster spreads networks over its nodes: no change in behavior",
};
const LXC: Support = Planned {
    note: "LXC containers: a way to run `docker:` machines on Proxmox",
};
const FIREWALL: Support = Planned {
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
                // The stable VM resource keeps its long name; the short `proxmox_vm` is experimental.
                _ if *r == "proxmox_virtual_environment_vm" => Emitted {
                    from: "machines (and the router)",
                },
                Some(c) if all.contains(&c.as_str()) => Tooling {
                    note: "older name of the resource without `virtual_environment_`",
                },
                _ => proxmox(r.strip_prefix(LEGACY).or_else(|| r.strip_prefix("proxmox_")).unwrap_or(r)).unwrap_or(Unclassified),
            };
            (format!("resource {r}"), s)
        })
        .collect();
    let mut all = vec![Format {
        name: "Terraform: Proxmox",
        file: ".isoloom/proxmox/ (planned)",
        source: "bpg/proxmox 0.115.0 (its resource types)",
        rows,
        collapse_not_portable: false,
    }];
    all.push(esxi());
    all.extend(CLOUDS.iter().map(cloud));
    all
}

fn proxmox(r: &str) -> Option<Support> {
    Some(match r {
        "vm" => Tooling {
            note: "an experimental VM resource; Isoloom uses proxmox_virtual_environment_vm",
        },
        "download_file" => Emitted {
            from: "machines.*.vm.os (cloud images)",
        },
        "file" => Emitted {
            from: "(cloud-init for each VM)",
        },
        "sdn_zone_simple" | "sdn_vnet" | "sdn_applier" => Emitted { from: "networks" },
        "pool" | "pool_membership" => GENERATOR,
        "cloned_vm" => Planned {
            note: "linked clones: faster starts from one image (no change in behavior)",
        },
        "sdn_subnet" => Equivalent {
            via: "the router VM, which holds each network's router address (a subnet would put it on the host bridge, which routes between networks)",
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

/// Standalone ESXi hosts (no vCenter), with the community provider josenk/esxi.
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
