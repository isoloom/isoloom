//! The `cloud-vm` target on clouds other than AWS: one cloud VM per machine, as Terraform under
//! `.isoloom/cloud-vm/<cloud>/`. AWS stays in [`super::cloud_vm`]; this module adds the rest and
//! shares that module's helpers (networks, the Linux set-up commands, the controller, the
//! outputs every cloud module gives, see docs/cloud-modules.md).
//!
//! Each cloud is a driver: `build` writes its `main.tf`, `refusal` says when the cloud can't
//! express a given spec (its module is then dropped for that spec, best-effort, rather than
//! failing the whole target). Azure and Google Cloud take static private addresses, so a lab's
//! fixed addressing and `reach` rules carry over faithfully. DigitalOcean, Linode and Oracle
//! Cloud don't pin private addresses the same way, so they take single-network Linux labs and
//! refuse the topologies they can't model.

use crate::model::Spec;

use super::GeneratedFile;

mod azure;
mod digitalocean;
mod gcp;
mod linode;
mod oci;

type Build = fn(&Spec) -> GeneratedFile;
type Refusal = fn(&Spec) -> Option<String>;

/// Every non-AWS cloud driver: its subdirectory, how to build it, and when it bows out. A cloud's
/// `refusal` drops only that cloud's module for a spec it can't model, best-effort.
pub(super) const DRIVERS: &[(&str, Build, Refusal)] = &[
    ("azure", azure::build, azure::refusal),
    ("gcp", gcp::build, gcp::refusal),
    ("digitalocean", digitalocean::build, digitalocean::refusal),
    ("linode", linode::build, linode::refusal),
    ("oci", oci::build, oci::refusal),
];
