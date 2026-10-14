//! The `cloud-docker` target: the environment's Compose file, run by Docker on one cloud VM, as
//! a Terraform module per cloud under `.isoloom/cloud-docker/<cloud>/`: AWS, Azure, Google
//! Cloud, DigitalOcean, Linode and Oracle Cloud. Rendered from the plan with the HCL builder.
//!
//! - The Compose file is the `docker` target's, rendered by the Compose backend
//!   (`.isoloom/docker/compose.yml` and the files it mounts): the VM runs exactly what
//!   `isoloom run docker` runs.
//! - Each cloud ([`Cloud`]) brings its resources: its own network, a firewall letting
//!   `allowed_cidr` in for SSH and the published ports, and one Debian (Ubuntu on Oracle) VM
//!   sized for every container at once ([`host::Sizing`]).
//! - The Docker host is set up over SSH through the VM core's `remote-exec` transport
//!   ([`host::setup`]): the plan's `Prelude` step (the constraints), the project's archive and
//!   the inputs file, Docker, `docker compose up --wait`, then the plan's `Ready` step
//!   (`/var/lib/isoloom/ready`).
//! - `auto_stop_minutes`: the VM shuts itself down after that long, so a forgotten environment
//!   stops costing.
//!
//! The same Docker host on a Proxmox server is `docker-vm`'s `proxmox` module ([`proxmox`]).

mod aws;
mod azure;
mod digitalocean;
mod gcp;
pub mod host;
mod linode;
mod oci;
pub mod proxmox;

use isoloom_plan::{Capabilities, Environment};
use isoloom_render::backend::{self, Backend, Files, RenderError};
use isoloom_render::{GenerateError, GeneratedFile};
use isoloom_spec::model::Target;

/// The target's folder under `.isoloom/`.
pub const DIR: &str = "cloud-docker";

/// A cloud the Docker host runs on: its folder under `.isoloom/cloud-docker/` and its module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cloud {
    /// Amazon Web Services.
    Aws,
    /// Microsoft Azure.
    Azure,
    /// Google Cloud.
    Gcp,
    /// DigitalOcean.
    DigitalOcean,
    /// Linode (Akamai).
    Linode,
    /// Oracle Cloud Infrastructure.
    Oci,
}

impl Cloud {
    /// Every cloud, in the order the modules are written.
    pub const ALL: [Cloud; 6] = [Cloud::Aws, Cloud::Azure, Cloud::Gcp, Cloud::DigitalOcean, Cloud::Linode, Cloud::Oci];

    /// Its folder.
    pub fn id(self) -> &'static str {
        match self {
            Cloud::Aws => "aws",
            Cloud::Azure => "azure",
            Cloud::Gcp => "gcp",
            Cloud::DigitalOcean => "digitalocean",
            Cloud::Linode => "linode",
            Cloud::Oci => "oci",
        }
    }

    /// Its module, from the plan.
    pub fn render(self, env: &Environment) -> Result<GeneratedFile, isoloom_render::hcl_builder::Error> {
        let m = match self {
            Cloud::Aws => aws::module(env),
            Cloud::Azure => azure::module(env),
            Cloud::Gcp => gcp::module(env),
            Cloud::DigitalOcean => digitalocean::module(env),
            Cloud::Linode => linode::module(env),
            Cloud::Oci => oci::module(env),
        };
        host::file(&m, DIR, self.id())
    }
}

/// The `cloud-docker` backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct CloudDockerBackend;

impl Backend for CloudDockerBackend {
    fn target(&self) -> Target {
        Target::CloudDocker
    }

    /// Compose's but secrets: the VM runs the Compose file, so it can render what Compose can,
    /// but `isoloom run` hands secrets only to a Compose it starts on this machine.
    fn capabilities(&self) -> Capabilities {
        isoloom_compose::render::capabilities_on_a_vm()
    }

    /// The `docker` target's Compose file (and the files it mounts), then a module per cloud.
    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        backend::check(Target::CloudDocker, env, &self.capabilities())?;
        if let Some(what) = refusal(env) {
            return Err(unsupported(what));
        }
        let on_docker = Environment::build(env.spec(), Target::Docker, env.instance);
        let mut files = isoloom_compose::render(Target::Docker, &on_docker)?;
        for cloud in Cloud::ALL {
            let module = cloud
                .render(env)
                .map_err(|e| unsupported(format!("the {} module couldn't be written: {e}", cloud.id())))?;
            files.push(module);
        }
        Ok(files)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &self.capabilities())
            .map(|r| r.to_string())
            .or_else(|| refusal(env))
    }
}

/// What the clouds can't host yet: arm64 (their images and VM sizes are x86-64).
pub fn refusal(env: &Environment) -> Option<String> {
    isoloom_render::arm64_machine(env.spec()).map(|name| format!("machine `{name}`: arm64 on a cloud VM comes later (Graviton instances and arm64 images)"))
}

fn unsupported(what: String) -> RenderError {
    RenderError::Generate(GenerateError::Unsupported {
        target: Target::CloudDocker,
        what,
    })
}

#[cfg(test)]
mod tests;
