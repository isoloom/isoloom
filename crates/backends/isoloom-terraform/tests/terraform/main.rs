//! The Terraform backends' modules as another crate renders them: each area's module beside this one.

mod aws_windows;
mod cloud_docker;
mod cloud_networks;
mod cloud_sizes;
mod controller_disk;
mod controller_inventory;
mod esxi;
mod ipv6;
mod own_names;
mod private_dns;
mod proxmox_links;
mod proxmox_ssh_proxy;
mod proxmox_virtiofs;
mod published_ports;
mod references;
mod server_regressions;
mod server_secrets;
mod set_up_once;
mod single_network_clouds;
mod snapshots;
mod testing;
mod user_data_caps;
mod volume_tags;
