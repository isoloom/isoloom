//! The target made of two backends, `cloud-docker`: the environment's containers run by Compose
//! on a cloud VM Terraform builds. It is put together here, above the backends, so no backend
//! depends on another: Compose writes the `docker` target's files, then Terraform a module per
//! cloud.

use isoloom_compose::ComposeBackend;
use isoloom_plan::{Capabilities, Environment};
use isoloom_render::Error as RenderError;
use isoloom_render::backend::{self, Backend, Files};
use isoloom_render::container::COMPOSE_OUTPUTS;
use isoloom_spec::model::Target;

/// `env` planned for the `docker` target: what a Docker host runs. It plans, since `env` did; a
/// failure is `env`'s target's.
fn on_docker(env: &Environment) -> Result<Environment, RenderError> {
    env.replan(env.spec(), Target::Docker, env.instance)
        .map_err(|e| RenderError::internal(env.target, "the containers couldn't be planned for Docker", e))
}

/// The `docker` target's files for `env`: what a Docker host runs. A refusal or failure is
/// `env`'s target's.
fn docker_files(env: &Environment) -> Result<Files, RenderError> {
    ComposeBackend::DOCKER.render(&on_docker(env)?).map_err(|e| e.on(env.target))
}

/// What the Docker host runs: the folders of the `docker` target's `files` it gets with the
/// project, and the commands that start the Compose file there as root, with the values they
/// read from their environment.
fn host(env: &Environment, files: &Files) -> Result<isoloom_terraform::CloudDockerHost, RenderError> {
    let in_vm = isoloom_compose::vm_start(&on_docker(env)?).map_err(|e| RenderError::from(e).on(env.target))?;
    let outputs = COMPOSE_OUTPUTS
        .iter()
        .filter(|dir| files.iter().any(|f| f.path.starts_with(&format!("{dir}/"))))
        .map(|dir| (*dir).to_owned())
        .collect();
    Ok(isoloom_terraform::CloudDockerHost {
        outputs,
        start: in_vm.start,
        values: in_vm.values,
    })
}

/// The `cloud-docker` backend: Compose on one cloud VM per cloud, built by Terraform.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CloudDockerBackend;

impl Backend for CloudDockerBackend {
    fn target(&self) -> Target {
        Target::CloudDocker
    }

    /// Compose's but existing networks: the VM runs the Compose file, so it can render what
    /// Compose can, with the secrets the module hands the VM; the Docker host is a fresh VM with
    /// no existing network to join.
    fn capabilities(&self) -> Capabilities {
        isoloom_compose::capabilities_on_a_vm()
    }

    /// The `docker` target's Compose file (and the files it mounts), then a module per cloud,
    /// whose host gets those files with the project and starts the Compose file from its user
    /// data.
    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        backend::check(Target::CloudDocker, env, &self.capabilities())?;
        let mut files = docker_files(env)?;
        let host = host(env, &files)?;
        files.extend(isoloom_terraform::cloud_docker_modules(env, &host)?);
        Ok(files)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &self.capabilities())
            .map(|r| r.to_string())
            .or_else(|| isoloom_terraform::cloud_docker_refusal(env))
    }
}
