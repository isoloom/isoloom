//! The Terraform backends: Proxmox, one cloud VM per machine (AWS, Azure, GCP, DigitalOcean,
//! Linode, OCI), Docker on one cloud VM, and cloud services. The VM targets share the VM core
//! ([`vm`]); Proxmox and AWS render from the plan through it.

pub mod cloud_docker;
pub mod cloud_services;
pub mod cloud_vm;
pub mod cloud_vm_others;
pub mod proxmox;
pub mod vm;

use isoloom_compose::docker;
use isoloom_render::{GenerateError, GeneratedFile, OUTPUT_DIR, arm64_machine, gcp_project_name, header};
use isoloom_spec::{model, shell};
