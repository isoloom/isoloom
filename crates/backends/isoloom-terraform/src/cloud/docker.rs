//! The Terraform half of the `cloud-docker` target: the environment's Compose file, run by
//! Docker on one cloud VM, as a module per cloud under `.isoloom/cloud-docker/<cloud>/`.
//!
//! - The Compose file is the `docker` target's (`.isoloom/docker/compose.yml` and the files it
//!   mounts), which `isoloom-targets` renders with the Compose backend before these modules: the
//!   VM runs exactly what the `docker` target runs.
//! - Each cloud brings its resources (`crate::cloud::<cloud>::docker`): its own network, a
//!   firewall letting `allowed_cidr` in for SSH and the published ports, and one Debian (Ubuntu
//!   on Oracle) VM sized for every container at once (`crate::docker_host::Sizing`).
//! - The Docker host is set up over SSH through the VM core's `remote-exec` transport
//!   (`crate::docker_host::setup`): the plan's `Prelude` step (the constraints), the project's
//!   archive and the inputs file, Docker, `docker compose up --wait`, then the plan's `Ready`
//!   step (`/var/lib/isoloom/ready`).
//! - `auto_stop_minutes`: the VM shuts itself down after that long, so a forgotten environment
//!   stops costing.
//!
//! The same Docker host on a Proxmox server is `docker-vm`'s module
//! (`crate::server::proxmox::docker_host`).

use isoloom_plan::Environment;
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Files;
use isoloom_spec::model::Target;

use super::{Cloud, aws, azure, digitalocean, gcp, linode, oci, operator};
use crate::module::{self, ProjectCopy, Usage};
use crate::{Error, Result};

/// Why the clouds can't host the environment's containers, if they can't: arm64 (their images
/// and VM sizes are x86-64).
#[must_use]
pub fn refusal(env: &Environment) -> Option<String> {
    isoloom_render::arm64_machine(env.spec()).map(|name| format!("machine `{name}`: arm64 on a cloud VM comes later (Graviton instances and arm64 images)"))
}

/// A Terraform module per cloud, in [`Cloud::ALL`]'s order: the Terraform side of the
/// `cloud-docker` target, after its Compose file.
///
/// # Errors
///
/// [`Error::Refused`] with the [`refusal`] when the clouds can't host the environment, or
/// [`Error::Module`] when a module can't be written.
pub fn modules(env: &Environment) -> Result<Files> {
    if let Some(reason) = refusal(env) {
        return Err(Error::Refused {
            target: Target::CloudDocker,
            reason,
        });
    }
    Cloud::ALL.into_iter().map(|cloud| module(cloud, env)).collect()
}

/// `cloud`'s module: how to start it, the cloud's resources, then the Docker host's set-up.
fn module(cloud: Cloud, env: &Environment) -> Result<GeneratedFile> {
    let folder = format!("{}/{}", Target::CloudDocker.id(), cloud.id());
    let mut m = module::start(&Usage::with_key(&folder, operator::ALLOWED));
    match cloud {
        Cloud::Aws => aws::docker::write(env, &mut m),
        Cloud::Azure => azure::docker::write(env, &mut m),
        Cloud::Gcp => gcp::docker::write(env, &mut m),
        Cloud::DigitalOcean => digitalocean::docker::write(env, &mut m),
        Cloud::Linode => linode::docker::write(env, &mut m),
        Cloud::Oci => oci::docker::write(env, &mut m),
    }
    module::file(Target::CloudDocker, &folder, m, ProjectCopy::Archive)
}

#[cfg(test)]
pub(crate) mod tests;
