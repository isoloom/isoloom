//! The targets made of two backends: the environment's containers run by Compose on a VM another
//! backend builds. They are put together here, above the backends, so no backend depends on
//! another: Compose writes the `docker` target's files (or the hybrid containers' Compose file),
//! and Terraform or Vagrant the VM around them.
//!
//! - `cloud-docker`: the `docker` target's files, then a Terraform module per cloud.
//! - `docker-vm`: the `docker` target's files, the Vagrantfile, then the Proxmox module (the
//!   cloud-docker Docker host, on a Proxmox server).
//! - `hybrid`: the Vagrant VMs and the containers' host, with the containers' Compose file.

use isoloom_plan::{Capabilities, Environment};
use isoloom_render::GenerateError;
use isoloom_render::backend::{self, Backend, Files, RenderError};
use isoloom_spec::model::Target;
use isoloom_vagrant::{docker_vm, hybrid};

/// The `docker` target's files for `env`: what a Docker host runs.
fn docker_files(env: &Environment) -> Result<Files, RenderError> {
    let on_docker = Environment::build(env.spec(), Target::Docker, env.instance);
    isoloom_compose::render(Target::Docker, &on_docker)
}

/// The `cloud-docker` backend: Compose on one cloud VM per cloud, built by Terraform.
#[derive(Debug, Clone, Copy, Default)]
pub struct CloudDockerBackend;

impl Backend for CloudDockerBackend {
    fn target(&self) -> Target {
        Target::CloudDocker
    }

    /// Compose's but existing networks: the VM runs the Compose file, so it can render what
    /// Compose can, with the secrets `isoloom run` delivers to the VM; the Docker host is a
    /// fresh VM with no existing network to join.
    fn capabilities(&self) -> Capabilities {
        isoloom_compose::render::capabilities_on_a_vm()
    }

    /// The `docker` target's Compose file (and the files it mounts), then a module per cloud.
    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        backend::check(Target::CloudDocker, env, &self.capabilities())?;
        let modules = isoloom_terraform::cloud_docker_modules(env)?;
        let mut files = docker_files(env)?;
        files.extend(modules);
        Ok(files)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &self.capabilities())
            .map(|r| r.to_string())
            .or_else(|| isoloom_terraform::cloud_docker_refusal(env))
    }
}

/// The `docker-vm` backend: Compose on one local VM (Vagrant), or on a Proxmox server
/// (Terraform).
#[derive(Debug, Clone, Copy, Default)]
pub struct DockerVmBackend;

impl Backend for DockerVmBackend {
    fn target(&self) -> Target {
        Target::DockerVm
    }

    /// Compose's but existing networks, as for `cloud-docker`: the VM runs the Compose file.
    fn capabilities(&self) -> Capabilities {
        isoloom_compose::render::capabilities_on_a_vm()
    }

    /// The `docker` target's Compose file (and the files it mounts), the Vagrantfile, and the
    /// Proxmox module.
    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        backend::check(Target::DockerVm, env, &self.capabilities())?;
        let mut files = docker_files(env)?;
        files.push(docker_vm::vagrantfile(env));
        files.push(isoloom_terraform::docker_vm_module(env)?);
        Ok(files)
    }
}

/// The `hybrid` backend: Vagrant VMs, and the containers on one more VM beside them.
#[derive(Debug, Clone, Copy, Default)]
pub struct HybridBackend;

impl Backend for HybridBackend {
    fn target(&self) -> Target {
        Target::Hybrid
    }

    fn capabilities(&self) -> Capabilities {
        hybrid::capabilities()
    }

    /// The Vagrantfile (the VMs, then the containers' host) and the checks next to it, and the
    /// containers' Compose file.
    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        backend::check(Target::Hybrid, env, &self.capabilities())?;
        let containers = isoloom_compose::hybrid::render(env)?;
        hybrid::render(env, containers)
    }

    /// The plan's refusal; else, since the VMs are still the `vagrant` generator's, what it
    /// refuses on rendering (a box it doesn't know, a Windows feature it lacks).
    fn refusal(&self, env: &Environment) -> Option<String> {
        if let Some(r) = isoloom_plan::refusal(&env.requirements, &self.capabilities()) {
            return Some(r.to_string());
        }
        match self.render(env).map_err(GenerateError::from) {
            Err(GenerateError::Unsupported { what, .. }) => Some(what),
            _ => None,
        }
    }
}
