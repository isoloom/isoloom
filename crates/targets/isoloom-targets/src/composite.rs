//! The targets made of two backends: the environment's containers run by Compose on a VM another
//! backend builds. They are put together here, above the backends, so no backend depends on
//! another: Compose writes the `docker` target's files (or the hybrid containers' Compose file),
//! and Terraform or Vagrant the VM around them.
//!
//! - `cloud-docker`: the `docker` target's files, then a Terraform module per cloud.
//! - `docker-vm`: the `docker` target's files, the Vagrantfile, then the Proxmox module (a Docker
//!   host VM on a Proxmox server, Terraform making only the VM, unless a container mounts a
//!   folder of the computer that runs the lab); and a run manifest for each
//!   (`docker-vm/run.json`, `docker-vm/proxmox/run.json`), the VM's steps from its backend, then
//!   the Compose file run inside it as the Compose backend says.
//! - `hybrid`: the Vagrant VMs and the containers' host, with the containers' Compose file.

use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::Error as RenderError;
use isoloom_render::backend::{self, Backend, Files, Placed};
use isoloom_render::container::host_folders;
use isoloom_render::run_manifest::guest::GuestCompose;
use isoloom_spec::model::Target;
use isoloom_vagrant::{docker_vm, hybrid};

/// `env` planned for the `docker` target: what a Docker host runs.
fn on_docker(env: &Environment) -> Environment {
    Environment::build(env.spec(), Target::Docker, env.instance)
}

/// The `docker` target's files for `env`: what a Docker host runs. A refusal or failure is
/// `env`'s target's.
fn docker_files(env: &Environment) -> Result<Files, RenderError> {
    isoloom_compose::render(Target::Docker, &on_docker(env)).map_err(|e| RenderError::from(e).on(env.target))
}

/// What runs inside the VM for `env`: the `docker` target's Compose file, and the commands that
/// start, check and read it there. A refusal or failure is `env`'s target's.
fn guest(env: &Environment) -> Result<GuestCompose, RenderError> {
    isoloom_compose::guest(&on_docker(env)).map_err(|e| RenderError::from(e).on(env.target))
}

/// The `cloud-docker` backend: Compose on one cloud VM per cloud, built by Terraform.
#[derive(Debug, Clone, Copy, Default)]
pub struct CloudDockerBackend;

impl Backend for CloudDockerBackend {
    fn target(&self) -> Target {
        Target::CloudDocker
    }

    /// Compose's but existing networks: the VM runs the Compose file, so it can render what
    /// Compose can, with the secrets the runner delivers to the VM; the Docker host is a
    /// fresh VM with no existing network to join.
    fn capabilities(&self) -> Capabilities {
        isoloom_compose::capabilities_on_a_vm()
    }

    /// The `docker` target's Compose file (and the files it mounts), a module per cloud, and the
    /// host's set-up script the runner pipes into each VM.
    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        backend::check(Target::CloudDocker, env, &self.capabilities())?;
        let mut files = docker_files(env)?;
        files.extend(isoloom_terraform::cloud_docker_modules(env)?);
        files.extend(isoloom_terraform::cloud_docker_setup(env)?);
        Ok(files)
    }

    /// A run manifest per cloud that can host the containers, built on the Compose-in-a-VM plan
    /// (`guest`, from the `docker` target's environment).
    fn run_manifests(&self, env: &Environment) -> Result<Vec<Placed>, RenderError> {
        Ok(isoloom_terraform::cloud_docker_manifests(env, &guest(env)?)?)
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

    /// Compose's but existing networks, as for `cloud-docker`: the VM runs the Compose file. And
    /// folders of the computer that runs the lab, which a local VM shares (`docker_vm_refusal`
    /// says which providers can't, `docker_vm_proxmox_refusal` that the server can't).
    fn capabilities(&self) -> Capabilities {
        isoloom_compose::capabilities_on_a_vm().with(Feature::HostFolders)
    }

    /// The `docker` target's Compose file (and the files it mounts), the Vagrantfile, and the
    /// Proxmox module with its set-up script, unless the containers mount folders of the
    /// computer that runs the lab (the server's VM runs elsewhere).
    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        backend::check(Target::DockerVm, env, &self.capabilities())?;
        let guest = guest(env)?;
        let mut files = docker_files(env)?;
        files.push(docker_vm::vagrantfile(env, &guest)?);
        if on_proxmox(env) {
            files.extend(isoloom_terraform::docker_vm_module(env, &guest)?);
        }
        Ok(files)
    }

    /// One manifest per way the VM is made: Vagrant's (`docker-vm/run.json`), Proxmox's
    /// (`docker-vm/proxmox/run.json`) when the lab can run there.
    fn run_manifests(&self, env: &Environment) -> Result<Vec<Placed>, RenderError> {
        let guest = guest(env)?;
        let mut out = vec![Placed::here(docker_vm::manifest(env, &guest)?)];
        if on_proxmox(env) {
            out.push(Placed::under("proxmox", isoloom_terraform::docker_vm_manifest(env, &guest)?));
        }
        Ok(out)
    }
}

/// Whether `docker-vm` writes its Proxmox module: unless the containers need a VM on the computer
/// that runs the lab (`docker_vm_proxmox_refusal`).
fn on_proxmox(env: &Environment) -> bool {
    docker_vm_proxmox_refusal(env).is_none()
}

/// Why `docker-vm` can't run on the Proxmox server, if it can't: a container mounts a folder of the
/// computer that runs the lab, which the server's VM, running on the server, can't share.
pub(crate) fn docker_vm_proxmox_refusal(env: &Environment) -> Option<String> {
    let name = host_folders::needing_a_local_vm(env)?;
    Some(format!(
        "machine `{name}` mounts a folder of the computer that runs the lab (`host_folders`): the Proxmox server's VM runs on the server, not on this computer"
    ))
}

/// The Proxmox server, as `docker-vm`'s variants name it.
pub(crate) const PROXMOX: &str = "proxmox";

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
        Ok(hybrid::render(env, containers)?)
    }

    /// The plan's refusal; else, since the VMs are still the `vagrant` generator's, what it
    /// refuses on rendering (a box it doesn't know, a Windows feature it lacks). A failure of
    /// the backend isn't a refusal: generating reports it.
    fn refusal(&self, env: &Environment) -> Option<String> {
        if let Some(r) = isoloom_plan::refusal(&env.requirements, &self.capabilities()) {
            return Some(r.to_string());
        }
        self.render(env).err().and_then(|e| e.refusal())
    }
}
