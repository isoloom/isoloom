//! The `cloud-vm` target on clouds other than AWS, each under `.isoloom/cloud-vm/<cloud>/` and
//! each a backend of its own rendered from the plan through the VM core (listed in
//! `super::cloud_vm::CLOUDS`). A cloud's module is dropped for a spec it can't model
//! (best-effort), rather than failing the whole target.
//!
//! Azure and Google Cloud take static private addresses, so a lab's fixed addressing and `reach`
//! rules carry over faithfully (Azure with Windows and machines on several networks, as AWS).
//! DigitalOcean, Linode and Oracle Cloud take Linux labs on the networks each can model
//! (`shared::Networks`) and refuse the topologies they can't (DigitalOcean, which can't pin
//! private addresses, one VM only; Linode, one network per machine).

pub mod azure;
pub mod digitalocean;
pub mod gcp;
pub mod linode;
pub mod oci;
mod shared;
