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

mod machines;
mod slots;
mod steps;

use std::collections::BTreeSet;

use isoloom_manifest::{
    Argv, Checks, Driver, Generator, GeneratorName, HostArgv, HostOs, Identity, Lab, LabName, Manifest, ManifestError, ManifestVersion, PinnedImage, Program,
    RelPath, SCHEMA_URL, StateOwner, Template, TerraformIdentity, Tool, ToolName, Version,
};
use isoloom_plan::Environment;
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
    Ok(Manifest {
        schema: Some(SCHEMA_URL.to_owned()),
        manifest_version: ManifestVersion,
        generator: Generator {
            name: GeneratorName::Isoloom,
            version: Version::parse(env!("CARGO_PKG_VERSION"))?,
        },
        lab: Lab {
            name: LabName::parse(&env.name)?,
            instance: env.instance.map(isoloom_manifest::Instance::new).transpose()?,
            message: None,
        },
        target: isoloom_manifest::Target::Proxmox,
        cloud: None,
        driver: Driver::Terraform,
        hosts: BTreeSet::from([HostOs::Linux, HostOs::Macos, HostOs::Windows]),
        tools: tools()?,
        identity: Identity::Terraform(TerraformIdentity {
            module: RelPath::parse(".")?,
            state: StateOwner::Runner,
            var_files: Vec::new(),
        }),
        params: slots::params(env, &runners)?,
        secrets: slots::secrets(env)?,
        networks: machines::networks(env)?,
        machines: machines::machines(env)?,
        lifecycle: steps::lifecycle()?,
        ready: steps::ready(env)?,
        outputs: machines::outputs(env)?,
        checks: Checks {
            protocol: isoloom_manifest::CheckProtocol::V1,
            setup: Vec::new(),
            runners,
            teardown: Vec::new(),
        },
        recovery: None,
        expiry: None,
    })
}

/// `program` with `args`, the same on every host.
fn argv(program: &str, args: &[&str]) -> Result<HostArgv, ManifestError> {
    Ok(HostArgv::Every(Argv::new(
        Program::parse(program)?,
        args.iter().map(|a| Template::text(a)).collect(),
    )))
}

/// Terraform (from its image on macOS) and `ssh`.
fn tools() -> Result<Vec<Tool>, ManifestError> {
    let tool = |name: &str, probe: HostArgv| -> Result<Tool, ManifestError> {
        Ok(Tool {
            name: ToolName::parse(name)?,
            probe,
            min_version: None,
            container: None,
            container_hosts: BTreeSet::new(),
            hosts: BTreeSet::new(),
            providers: BTreeSet::new(),
        })
    };
    let mut terraform = tool("terraform", argv("terraform", &["version"])?)?;
    terraform.min_version = Some(Version::parse(TERRAFORM_MIN)?);
    terraform.container = Some(PinnedImage::parse(TERRAFORM_IMAGE)?);
    terraform.container_hosts = BTreeSet::from([HostOs::Macos]);
    Ok(vec![terraform, tool("ssh", argv("ssh", &["-V"])?)?])
}

#[cfg(test)]
mod tests;
