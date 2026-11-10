//! A server's run manifest (`.isoloom/proxmox/run.json`, `.isoloom/esxi/run.json`): what the
//! runner executes, built from the plan by pure functions, the same on both servers but for what
//! [`Server`] says.
//!
//! - **Terraform makes the environment.** The module is applied in place, its state kept by the
//!   runner (`identity.terraform.state: runner`). Each Linux machine says how far its set-up got
//!   in its ready file (`isoloom-ready/1`): on Proxmox it sets itself up at boot from its
//!   cloud-init, on ESXi Terraform sets it up over SSH during the apply. The runner reads each
//!   ready file and runs the checks over SSH, through the router (`ssh.run`), so the same steps run
//!   from macOS, Windows and Linux.
//! - **The seal** (`lifecycle.seal`), when the run has an `internet: false` network: once the
//!   machines are ready, one `ssh.run` applies the router's sealed ruleset live
//!   (`crate::server::router`).
//! - **Params are the module's variables**, the connection's secrets are slots, the inputs one map
//!   (`inputs`). The run's own values come from the runner, in the steps' environment
//!   (`steps::runner_set`): its slot on the server (`${run.slot}`), its key (`${run.ssh_public_key}`,
//!   and its private half's file `${run.ssh_key}` as [`Server::private_key`]).

mod machines;
pub(super) mod slots;
pub(super) mod steps;

use isoloom_manifest::{Checks, Driver, Error as ManifestError, GoneError, Manifest, ManifestVersion, SCHEMA_URL, Template, Tool, Version};
use isoloom_plan::Environment;
use isoloom_render::run_manifest::{self, environment, generator};
use isoloom_spec::model::Target;

use crate::Error;
use crate::module::variable::Variable;

/// What a server's run manifest takes from its target; the rest is the same on every server.
pub(crate) struct Server<'a> {
    /// The target.
    pub(crate) target: Target,
    /// The target, as the manifest names it.
    pub(crate) names: isoloom_manifest::Target,
    /// The module's variables: the params and the connection's slots among them.
    pub(crate) variables: Vec<Variable>,
    /// The module's variable the runner gives its private key's file (`${run.ssh_key}`): what
    /// Proxmox uploads the snippets with, what ESXi's Terraform sets the machines up with.
    pub(crate) private_key: &'static str,
    /// The tools a run needs.
    pub(crate) tools: fn() -> Result<Vec<Tool>, ManifestError>,
    /// Network `net`'s own name on the server (its VNet, its port group).
    pub(crate) native: &'a dyn Fn(&str) -> Template,
    /// What a successful apply means of the machines: how far their set-up is then.
    pub(crate) applied: &'static str,
    /// The file on each Linux machine its set-up writes its output to, whose tail explains a
    /// failed step; `None` when the set-up's output is the apply's own.
    pub(crate) setup_log: Option<&'static str>,
    /// The errors of the server's provider for a resource already gone (`identity.terraform.gone`).
    pub(crate) gone: fn() -> Result<Vec<GoneError>, ManifestError>,
}

/// The run manifest of `env` on `server`; its message is left to the caller.
pub(crate) fn manifest(env: &Environment, server: &Server) -> crate::Result<Manifest> {
    build(env, server).map_err(|source| Error::Manifest { target: server.target, source })
}

fn build(env: &Environment, server: &Server) -> Result<Manifest, ManifestError> {
    let runners = steps::runners(env)?;
    let machines = machines::machines(env)?;
    let sealed = crate::seal::any(env);
    Ok(Manifest {
        schema: Some(SCHEMA_URL.to_owned()),
        manifest_version: ManifestVersion,
        generator: generator()?,
        environment: environment(env)?,
        target: server.names,
        cloud: None,
        driver: Driver::Terraform,
        hosts: run_manifest::every_host(),
        tools: (server.tools)()?,
        identity: crate::module::manifest::runner_identity((server.gone)()?)?,
        params: slots::params(&server.variables, server.private_key, &runners, sealed)?,
        secrets: slots::secrets(env, &server.variables)?,
        networks: run_manifest::networks(env, server.native)?,
        outputs: machines::outputs(&machines)?,
        machines,
        lifecycle: steps::lifecycle(server.private_key, sealed)?,
        ready: steps::ready(env, server.applied, server.setup_log)?,
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

/// Terraform, at the oldest release every module Isoloom writes takes.
pub(super) fn terraform() -> Result<Tool, ManifestError> {
    let mut terraform = run_manifest::tool("terraform", run_manifest::argv("terraform", &["version"])?)?;
    terraform.min_version = Some(Version::parse(crate::module::providers::TERRAFORM_MIN)?);
    Ok(terraform)
}

/// `ssh`, which reaches every machine through the router.
pub(super) fn ssh() -> Result<Tool, ManifestError> {
    run_manifest::tool("ssh", run_manifest::argv("ssh", &["-V"])?)
}

#[cfg(test)]
mod tests;
