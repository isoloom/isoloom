//! The registry of backends, one per generated target. Compose and external render the plan; the targets
//! not migrated yet go through [`LegacyBackend`], which runs today's generator on the spec the
//! plan was built from, with the text post-passes it still needs, so their output is unchanged.

use isoloom_compose::ComposeBackend;
use isoloom_external::ExternalBackend;
use isoloom_kubernetes::kubernetes;
use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_render::{GenerateError, constraints, project};
use isoloom_spec::model::{Spec, Target};
use isoloom_terraform::{cloud_docker, cloud_services, cloud_vm, proxmox};
use isoloom_vagrant::{docker_vm, hybrid, vagrant};

/// A generator that predates the plan: a pure function of the spec.
type Generator = fn(&Spec) -> Result<Files, GenerateError>;

/// A target whose generator reads the spec directly. Its refusals come from generating (as
/// before the plan), except for network appliances, which only the Docker targets wire.
pub struct LegacyBackend {
    target: Target,
    generate: Generator,
}

impl LegacyBackend {
    /// Whether the target runs Docker's networks, so it can wire network appliances.
    fn docker_family(&self) -> bool {
        matches!(self.target, Target::DockerVm | Target::CloudDocker)
    }
}

impl Backend for LegacyBackend {
    fn target(&self) -> Target {
        self.target
    }

    /// Everything but appliances off Docker: the generator itself refuses what it can't do.
    fn capabilities(&self) -> Capabilities {
        let all = [Feature::EnvironmentProvisioning, Feature::PlaybookChecks, Feature::ExecWithoutContainer];
        let appliances = self.docker_family().then_some(Feature::NetworkAppliances);
        Capabilities::of(all.into_iter().chain(appliances))
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        isoloom_render::backend::check(self.target, env, &self.capabilities())?;
        let spec = env.spec();
        let mut files = (self.generate)(spec)?;
        for f in files.iter_mut().filter(|f| f.path.ends_with(".tf")) {
            project::terraform(&mut f.contents);
        }
        constraints::rewrite_images(spec, &mut files);
        constraints::terraform_prelude(spec, &mut files);
        Ok(files)
    }

    /// Generates, as before the plan: the generator's own refusal, if any.
    fn refusal(&self, env: &Environment) -> Option<String> {
        match self.render(env) {
            Err(e) => match GenerateError::from(e) {
                GenerateError::Unsupported { what, .. } => Some(what),
                _ => None,
            },
            Ok(_) => None,
        }
    }
}

/// The Compose file of a Docker-on-a-VM target: the `docker` target's, from its own plan.
fn compose_of(spec: &Spec) -> Result<Files, GenerateError> {
    let env = Environment::build(spec, Target::Docker, None);
    isoloom_compose::render(Target::Docker, &env).map_err(GenerateError::from)
}

fn docker_vm_files(spec: &Spec) -> Result<Files, GenerateError> {
    let mut files = compose_of(spec)?;
    files.extend(docker_vm::generate(spec)?);
    Ok(files)
}

fn cloud_docker_files(spec: &Spec) -> Result<Files, GenerateError> {
    let mut files = compose_of(spec)?;
    files.extend(cloud_docker::generate(spec)?);
    Ok(files)
}

const fn legacy(target: Target, generate: Generator) -> LegacyBackend {
    LegacyBackend { target, generate }
}

static DOCKER: ComposeBackend = ComposeBackend(Target::Docker);
static HOSTED: ComposeBackend = ComposeBackend(Target::Hosted);
static DOCKER_VM: LegacyBackend = legacy(Target::DockerVm, docker_vm_files);
static CLOUD_DOCKER: LegacyBackend = legacy(Target::CloudDocker, cloud_docker_files);
static KUBERNETES: LegacyBackend = legacy(Target::Kubernetes, kubernetes::generate);
static HYBRID: LegacyBackend = legacy(Target::Hybrid, hybrid::generate);
static VAGRANT: LegacyBackend = legacy(Target::Vagrant, vagrant::generate);
static PROXMOX: LegacyBackend = legacy(Target::Proxmox, proxmox::generate);
static CLOUD_VM: LegacyBackend = legacy(Target::CloudVm, cloud_vm::generate);
static EXTERNAL: ExternalBackend = ExternalBackend;
static CLOUD_SERVICES: LegacyBackend = legacy(Target::CloudServices, cloud_services::generate);

/// Every backend, in the order targets are listed.
pub static BACKENDS: &[&dyn Backend] = &[
    &DOCKER,
    &HOSTED,
    &DOCKER_VM,
    &CLOUD_DOCKER,
    &KUBERNETES,
    &HYBRID,
    &VAGRANT,
    &PROXMOX,
    &CLOUD_VM,
    &EXTERNAL,
    &CLOUD_SERVICES,
];

/// The backend of `target`, if it has one.
pub fn backend(target: Target) -> Option<&'static dyn Backend> {
    BACKENDS.iter().copied().find(|b| b.target() == target)
}
