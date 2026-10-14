//! The `cloud-vm` target on clouds other than AWS: one cloud VM per machine, as Terraform under
//! `.isoloom/cloud-vm/<cloud>/`. AWS stays in [`super::cloud_vm`]; this module adds the rest and
//! shares that module's helpers (networks, the Linux set-up commands, the controller, the
//! outputs every cloud module gives, see docs/cloud-modules.md).
//!
//! A cloud's module is dropped for a spec it can't model (best-effort), rather than failing the
//! whole target. Azure and Google Cloud take static private addresses, so a lab's fixed
//! addressing and `reach` rules carry over faithfully; they are drivers still reading the spec:
//! `build` writes their `main.tf`, `refusal` says when they bow out. DigitalOcean, Linode and
//! Oracle Cloud are backends rendered from the plan through the VM core (listed in
//! `super::cloud_vm::CLOUDS`); they take single-network Linux labs and refuse the topologies
//! they can't model (DigitalOcean, which can't pin private addresses, one VM only).

use crate::model::Spec;

use super::GeneratedFile;

mod azure;
pub mod digitalocean;
mod gcp;
pub mod linode;
pub mod oci;
mod single_network;

type Build = fn(&Spec) -> GeneratedFile;
type Refusal = fn(&Spec) -> Option<String>;

/// The non-AWS clouds still reading the spec (the others are backends: `super::cloud_vm::CLOUDS`):
/// its subdirectory, how to build it, and when it bows out. A cloud's
/// `refusal` drops only that cloud's module for a spec it can't model, best-effort.
pub const DRIVERS: &[(&str, Build, Refusal)] = &[("azure", azure::build, azure::refusal), ("gcp", gcp::build, gcp::refusal)];
