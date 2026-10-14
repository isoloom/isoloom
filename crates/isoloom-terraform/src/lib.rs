//! The Terraform backends: Proxmox, one cloud VM per machine (AWS, Azure, GCP, DigitalOcean,
//! Linode, OCI), Docker on one cloud VM, and cloud services.

pub mod cloud_docker;
pub mod cloud_services;
pub mod cloud_vm;
pub mod cloud_vm_others;
pub mod proxmox;

use isoloom_compose::docker;
use isoloom_plan::checks;
use isoloom_render::{
    GenerateError, GeneratedFile, OUTPUT_DIR, address, address_for, arm64_machine, controller_size, gcp_project_name, hcl, header, host_vars, machine_setup,
    names_of, router, start_order, trunks,
};
use isoloom_spec::{groups, images, model, shell, validate};
