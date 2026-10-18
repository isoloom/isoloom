//! The `proxmox` target's run manifest (`.isoloom/proxmox/run.json`): what the Isothrum runner
//! executes, built from the plan by pure functions.
//!
//! - **Terraform keeps only the infrastructure.** The module is applied in place, its state kept by
//!   the runner (`identity.terraform.state: runner`); each machine sets itself up at boot from its
//!   cloud-init, and says how far it got in its ready file (`isoloom-ready/1`). Nothing reaches a
//!   guest from Terraform: the runner reads each ready file and runs the checks over SSH, through
//!   the router (`ssh.run`), so the same steps run from macOS, Windows and Linux.
//! - **Params are the module's variables**, the connection's secrets are slots, the inputs one map
//!   (`inputs`). The run's own values come from the runner: its slot on the server
//!   (`${run.slot}`), its key (`${run.ssh_public_key}`, and `${run.ssh_key}` for the snippet
//!   upload with an API token).
//! - **Tools:** Terraform, run from its pinned image on macOS (Local Network Privacy keeps a host
//!   binary's provider off the LAN), and `ssh`.

pub(super) mod docker_vm;
mod machines;
mod slots;
mod steps;

use std::collections::BTreeSet;

use isoloom_manifest::{
    Checks, Driver, Error as ManifestError, HostOs, Identity, Manifest, ManifestVersion, PinnedImage, RelPath, SCHEMA_URL, StateOwner, TerraformIdentity, Tool,
    Version,
};
use isoloom_plan::Environment;
use isoloom_render::run_manifest::{argv, environment, generator, tool};
use isoloom_spec::model::Target;

use crate::Error;

/// The Terraform image macOS runs the module from, pinned by the digest of its multi-arch index
/// (the launcher's, proven on a Proxmox server).
const TERRAFORM_IMAGE: &str = "hashicorp/terraform:1.9.8@sha256:18f9986038bbaf02cf49db9c09261c778161c51dcc7fb7e355ae8938459428cd";

/// The oldest Terraform the module takes (its `required_version`).
const TERRAFORM_MIN: &str = "1.6.0";

/// The run manifest of `env` on Proxmox; its message is left to the caller.
pub(super) fn manifest(env: &Environment) -> crate::Result<Manifest> {
    build(env).map_err(|source| Error::Manifest {
        target: Target::Proxmox,
        source,
    })
}

fn build(env: &Environment) -> Result<Manifest, ManifestError> {
    let runners = steps::runners(env)?;
    let machines = machines::machines(env)?;
    Ok(Manifest {
        schema: Some(SCHEMA_URL.to_owned()),
        manifest_version: ManifestVersion,
        generator: generator()?,
        environment: environment(env)?,
        target: isoloom_manifest::Target::Proxmox,
        cloud: None,
        driver: Driver::Terraform,
        hosts: every_host(),
        tools: tools()?,
        identity: runner_identity()?,
        params: slots::params(&super::base::variables(env), &runners, crate::cloud::vm::seal::any(env))?,
        secrets: slots::secrets(env)?,
        networks: machines::networks(env)?,
        outputs: machines::outputs(&machines)?,
        machines,
        lifecycle: steps::lifecycle(crate::cloud::vm::seal::any(env))?,
        ready: steps::ready(env)?,
        checks: Checks {
            protocol: isoloom_manifest::CheckProtocol::V1,
            setup: Vec::new(),
            runners,
            teardown: Vec::new(),
        },
        recovery: None,
        expiry: None,
        guest: None,
    })
}

/// Every host a Terraform target runs from.
pub(super) fn every_host() -> BTreeSet<HostOs> {
    BTreeSet::from([HostOs::Linux, HostOs::Macos, HostOs::Windows])
}

/// The module in place, its state the runner's.
pub(super) fn runner_identity() -> Result<Identity, ManifestError> {
    Ok(Identity::Terraform(TerraformIdentity {
        module: RelPath::parse(".")?,
        state: StateOwner::Runner,
        var_files: Vec::new(),
    }))
}

/// Terraform (from its image on macOS) and `ssh`.
pub(super) fn tools() -> Result<Vec<Tool>, ManifestError> {
    let mut terraform = tool("terraform", argv("terraform", &["version"])?)?;
    terraform.min_version = Some(Version::parse(TERRAFORM_MIN)?);
    terraform.container = Some(PinnedImage::parse(TERRAFORM_IMAGE)?);
    terraform.container_hosts = BTreeSet::from([HostOs::Macos]);
    Ok(vec![terraform, tool("ssh", argv("ssh", &["-V"])?)?])
}

#[cfg(test)]
mod tests;
