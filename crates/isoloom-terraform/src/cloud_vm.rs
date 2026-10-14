//! The `cloud-vm` target: one cloud VM per machine, a Terraform module per cloud under
//! `.isoloom/cloud-vm/<cloud>/`, and the check runners every cloud shares under
//! `.isoloom/cloud-vm/checks/`.
//!
//! Each cloud is a backend of its own ([`CLOUDS`]: AWS, Azure, Google Cloud, DigitalOcean,
//! Linode, Oracle Cloud): its capabilities, its refusal read from the plan, its module, rendered
//! from the plan through the VM core (`crate::vm`). No cloud stands for the others: a cloud whose
//! refusal fires drops only its module, so a feature one cloud can't do never hides another
//! cloud's working module. The target is refused only when every cloud refuses it.
//! [`cloud_refusals`] says which clouds refuse and why; `isoloom run cloud-vm --cloud <name>`
//! picks the module to apply (ADR 0007).

pub mod aws;

pub use aws::{image_user, instance_type};

use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::GenerateError;
use isoloom_render::backend::{self, Backend, Files, RenderError};
use isoloom_spec::model::Target;

use crate::cloud_vm_others::azure::AzureBackend;
use crate::cloud_vm_others::digitalocean::DigitalOceanBackend;
use crate::cloud_vm_others::gcp::GcpBackend;
use crate::cloud_vm_others::linode::LinodeBackend;
use crate::cloud_vm_others::oci::OciBackend;
use crate::vm::checks;

/// The target's folder under `.isoloom/`.
pub const DIR: &str = "cloud-vm";

/// Every cloud on the plan, by its folder under `.isoloom/cloud-vm/`, AWS first.
pub static CLOUDS: &[(&str, &dyn Backend)] = &[
    ("aws", &AwsBackend),
    ("azure", &AzureBackend),
    ("gcp", &GcpBackend),
    ("digitalocean", &DigitalOceanBackend),
    ("linode", &LinodeBackend),
    ("oci", &OciBackend),
];

/// Each cloud on the plan, AWS first, with why it can't run the environment (`None`: its module
/// is written).
pub fn cloud_refusals(env: &Environment) -> Vec<(&'static str, Option<String>)> {
    CLOUDS.iter().map(|(name, b)| (*name, b.refusal(env))).collect()
}

/// What the clouds of the target can render: environment-level provisioning (a controller),
/// playbook checks (refused by each cloud's own limit until the cloud runs them), `exec` checks
/// (run on the machine), machine aliases and ulimits (the set-up steps), and gateway networks
/// and link impairment (refused by each cloud's own limits, with their own reasons: the plan's
/// requirements name other targets).
pub fn capabilities() -> Capabilities {
    Capabilities::of([
        Feature::EnvironmentProvisioning,
        Feature::PlaybookChecks,
        Feature::ExecWithoutContainer,
        Feature::Gateways,
        Feature::MachineAliases,
        Feature::Ulimits,
        Feature::TrafficControl,
        Feature::Ipv6,
        // Given on AWS (NitroTPM), Azure, Google Cloud and Oracle Cloud; DigitalOcean and Linode
        // refuse it on their own.
        Feature::Tpm,
        // Delivered by `isoloom run` over the machine's SSH, owners included.
        Feature::Secrets,
        Feature::SecretOwners,
        Feature::MachineProcess,
        // A GPU instance type (each cloud's own table; refused where it has none).
        Feature::VmGpus,
        // A size that nests and the cloud's switch (refused by the clouds that have none).
        Feature::NestedVirtualization,
    ])
}

/// A cloud's refusal, `why`, as a rendering error.
pub(crate) fn unsupported(why: String) -> RenderError {
    RenderError::Generate(GenerateError::Unsupported {
        target: Target::CloudVm,
        what: why,
    })
}

/// A cloud's `render`: the plan's requirements, then its own refusal, then its module.
pub(crate) fn render_cloud(
    env: &Environment,
    refusal: fn(&Environment) -> Option<String>,
    render: fn(&Environment) -> Result<isoloom_render::GeneratedFile, isoloom_render::hcl_builder::Error>,
) -> Result<Files, RenderError> {
    backend::check(Target::CloudVm, env, &capabilities())?;
    if let Some(why) = refusal(env) {
        return Err(unsupported(why));
    }
    Ok(vec![render(env).map_err(|e| unsupported(format!("the module couldn't be written: {e}")))?])
}

/// A cloud's `refusal`: the plan's requirements, then its own.
pub(crate) fn refuse_cloud(env: &Environment, refusal: fn(&Environment) -> Option<String>) -> Option<String> {
    isoloom_plan::refusal(&env.requirements, &capabilities())
        .map(|r| r.to_string())
        .or_else(|| refusal(env))
}

/// Why no cloud can run the environment, when none can: the plan's requirements the target
/// lacks, else each cloud's own reason, the clouds sharing one named together. `None` as soon as
/// one cloud accepts it.
pub fn refusal(env: &Environment) -> Option<String> {
    if let Some(r) = isoloom_plan::refusal(&env.requirements, &capabilities()) {
        return Some(r.to_string());
    }
    let mut reasons: Vec<(Vec<&str>, String)> = Vec::new();
    for (name, why) in cloud_refusals(env) {
        let why = why?;
        match reasons.iter_mut().find(|(_, r)| *r == why) {
            Some((clouds, _)) => clouds.push(name),
            None => reasons.push((vec![name], why)),
        }
    }
    let parts: Vec<String> = reasons.into_iter().map(|(clouds, why)| format!("{}: {why}", clouds.join(", "))).collect();
    Some(format!("no cloud can run it ({})", parts.join("; ")))
}

/// The `cloud-vm` backend: a module for every cloud whose model fits the environment.
#[derive(Debug, Clone, Copy, Default)]
pub struct CloudVmBackend;

impl Backend for CloudVmBackend {
    fn target(&self) -> Target {
        Target::CloudVm
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        backend::check(Target::CloudVm, env, &capabilities())?;
        if let Some(why) = refusal(env) {
            return Err(unsupported(why));
        }
        let mut files = Vec::new();
        for (_, cloud) in CLOUDS {
            if cloud.refusal(env).is_none() {
                files.extend(cloud.render(env)?);
            }
        }
        files.extend(checks::runners(env, DIR));
        Ok(files)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        refusal(env)
    }
}

/// The `cloud-vm` module on AWS.
#[derive(Debug, Clone, Copy, Default)]
pub struct AwsBackend;

impl Backend for AwsBackend {
    fn target(&self) -> Target {
        Target::CloudVm
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        render_cloud(env, aws::refusal, aws::render)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        refuse_cloud(env, aws::refusal)
    }
}
