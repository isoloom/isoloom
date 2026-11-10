//! The `proxmox` target's run manifest (`.isoloom/proxmox/run.json`), the servers' own
//! (`crate::server::manifest`) with what is Proxmox's:
//!
//! - **The runner's private key** (`${run.ssh_key}`) for the snippets' upload over SSH with an API
//!   token.
//! - **Tools:** Terraform, run from its pinned image on macOS (Local Network Privacy keeps a host
//!   binary's provider off the LAN), and `ssh`.
//! - **Networks** named after the VNet (the node's bridge) each is, or whose VLAN it is.

pub(super) mod docker_vm;

use std::collections::{BTreeMap, BTreeSet};

use isoloom_manifest::{Error as ManifestError, HostOs, Manifest, PinnedImage, Tool};
use isoloom_plan::Environment;
use isoloom_spec::model::Target;

use crate::server::manifest::{self as shared, Server};

/// The Terraform image macOS runs the module from, pinned by the digest of its multi-arch index:
/// a release new enough for every module Isoloom writes ([`TERRAFORM_MIN`](crate::module::providers::TERRAFORM_MIN) or later).
const TERRAFORM_IMAGE: &str = "hashicorp/terraform:1.16.7@sha256:c9c77f5b9820832cbc7b88d16df406ac6e75fee5a9077e5437db0c54e5ff33e3";

/// The module's variable the runner gives its private key's file: the snippets' upload over SSH.
pub(super) const PRIVATE_KEY: &str = "proxmox_ssh_private_key_file";

/// The run manifest of `env` on Proxmox; its message is left to the caller.
pub(super) fn manifest(env: &Environment) -> crate::Result<Manifest> {
    let vnets: BTreeMap<&str, String> = super::links::vnet_ids(env).into_iter().collect();
    let native = |net: &str| {
        let on = env.networks.get(net).and_then(|n| n.vlan.as_ref().map(|(l, _)| l.as_str())).unwrap_or(net);
        shared::slotted("i", vnets.get(on).cloned().unwrap_or_default())
    };
    let server = Server {
        target: Target::Proxmox,
        names: isoloom_manifest::Target::Proxmox,
        variables: super::base::variables(env),
        private_key: PRIVATE_KEY,
        tools,
        native: &native,
        applied: "the VMs exist and booted; each sets itself up from its cloud-init",
    };
    shared::manifest(env, &server)
}

/// Terraform (from its image on macOS) and `ssh`.
pub(super) fn tools() -> Result<Vec<Tool>, ManifestError> {
    let mut terraform = shared::terraform()?;
    terraform.container = Some(PinnedImage::parse(TERRAFORM_IMAGE)?);
    terraform.container_hosts = BTreeSet::from([HostOs::Macos]);
    Ok(vec![terraform, shared::ssh()?])
}

#[cfg(test)]
mod tests;
