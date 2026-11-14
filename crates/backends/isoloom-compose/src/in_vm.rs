//! The `docker` target's Compose file run inside a Docker host VM (`cloud-docker`): the commands
//! that start it from the VM's copy of the project, as root, and the variables those commands
//! read from their environment. The VM's backend runs them from the VM's own set-up.

use isoloom_plan::{Environment, Image, MachineShape};
use isoloom_render::container::{host_environment, host_folders};
use isoloom_render::steps::PROJECT_DIR;
use isoloom_spec::contract::{OUTPUT_DIR, PUBLISH_ADDRESS, PUBLISH_FIXED};

use crate::error::Result;
use crate::backend::ComposeTarget;
use crate::layout::DIR;
use crate::render;

/// How long `up --wait` waits for the services to be healthy, in seconds.
const WAIT_TIMEOUT: &str = "600";

/// What starts the Compose file in a Docker host VM.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct VmStart {
    /// The commands, run as root in order: the published images pulled first when a machine
    /// both builds and publishes its image (a failed pull tolerated: `up` builds it), then
    /// `docker compose … up --wait`, which fails when a service isn't healthy or a set-up job
    /// fails.
    pub start: Vec<String>,
    /// The variables those commands read from their environment: each input a container reads,
    /// under its name, and each secret a container gets (`ISOLOOM_SECRET_<NAME>`, an input's
    /// file its input's own), sorted, each once.
    pub values: Vec<String>,
}

/// How `env` (planned for the `docker` target) starts in a Docker host VM.
///
/// # Errors
///
/// [`crate::Error::Refused`] when `env` needs what Compose can't do.
pub fn vm_start(env: &Environment) -> Result<VmStart> {
    let file = render::compose_file(ComposeTarget::Docker, env)?.file;
    let folders: String = host_folders::of_containers(env)
        .iter()
        .map(|f| format!("{}={} ", f.variable, f.in_vm))
        .collect();
    let compose = format!(
        "cd {PROJECT_DIR} && {}env {PUBLISH_ADDRESS}=0.0.0.0 {PUBLISH_FIXED}=1 {folders}docker compose --project-name {} --file {OUTPUT_DIR}/{DIR}/compose.yml",
        host_environment(env),
        file.name
    );
    let pull_published = publishes_a_built_image(env);
    let mut start = Vec::new();
    if pull_published {
        // A service with no published image, or one not pushed yet, is built by `up`.
        start.push(format!("{compose} pull --ignore-pull-failures || true"));
    }
    let build = if pull_published { "" } else { " --build" };
    start.push(format!(
        "{compose} up --detach{build} --pull missing --remove-orphans --wait --wait-timeout {WAIT_TIMEOUT}"
    ));
    Ok(VmStart { start, values: values(env) })
}

/// Whether any machine runs a published image it also builds (the build+image combo): the start
/// pulls before building, so a user runs the published image.
fn publishes_a_built_image(env: &Environment) -> bool {
    env.machines.values().any(|mp| {
        matches!(
            &mp.image,
            Some(Image::Container {
                build: Some(_),
                published: true,
                ..
            })
        )
    })
}

/// The inputs the containers read and the secrets they get, as the variables the Compose file
/// reads them from.
fn values(env: &Environment) -> Vec<String> {
    let containers = || env.machines.values().filter(|m| m.shape == MachineShape::Container);
    let mut out: Vec<String> = env
        .inputs
        .iter()
        .filter(|i| containers().any(|m| m.reads_input(&i.name)))
        .map(|i| i.name.clone())
        .collect();
    out.extend(
        isoloom_render::secrets::of_containers(env)
            .iter()
            .map(|name| isoloom_render::secrets::compose_variable(env, name)),
    );
    out.sort();
    out.dedup();
    out
}
