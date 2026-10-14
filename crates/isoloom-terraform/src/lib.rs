//! The Terraform backends: Proxmox, one cloud VM per machine (AWS, Azure, GCP, DigitalOcean,
//! Linode, OCI), Docker on one cloud VM, and cloud services, all rendered from the plan. The VM
//! targets share the VM core ([`vm`]).

pub mod cloud_docker;
pub mod cloud_services;
pub mod cloud_vm;
pub mod cloud_vm_others;
pub mod proxmox;
pub mod vm;
