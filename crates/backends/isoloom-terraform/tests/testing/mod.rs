//! What the integration tests share: specs from the repository's examples or written in a
//! test, checked valid, and the file a backend wrote at a path.

use std::error::Error;
use std::path::Path;

use isoloom_plan::{Environment, Lab};
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule, ProxmoxBackend};

/// A test's outcome: a failure is the report.
pub type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The repository's example `name` (its folder under `examples/`), checked valid.
///
/// # Errors
///
/// When it doesn't load, or isn't valid.
pub fn example(name: &str) -> Outcome<Lab> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name);
    valid(Lab::new(isoloom_spec::load(&dir)?))
}

/// The spec `yaml` (the authored format), checked valid.
///
/// # Errors
///
/// When it doesn't parse, or isn't valid.
pub fn spec(yaml: &str) -> Outcome<Lab> {
    valid(parsed(yaml)?)
}

/// The plan of `yaml`, a valid spec, for `target`.
///
/// # Errors
///
/// When the spec doesn't parse, or isn't valid.
pub fn plan(yaml: &str, target: Target) -> Outcome<Environment> {
    Ok(planned(&spec(yaml)?, target))
}

/// The plan of `lab` for `target`: its spec lowered for the target, then planned.
#[must_use]
pub fn planned(lab: &Lab, target: Target) -> Environment {
    Environment::build(&lab.for_target(target), target, None)
}

/// `cloud`'s `cloud-vm` module for `yaml`, a valid spec.
///
/// # Errors
///
/// When the spec isn't valid, or the cloud refuses it.
pub fn cloud_vm(cloud: Cloud, yaml: &str) -> Outcome<String> {
    Ok(CloudVmModule::new(cloud).render(&plan(yaml, Target::CloudVm)?)?.contents)
}

/// The Proxmox module for `yaml`, a valid spec.
///
/// # Errors
///
/// When the spec isn't valid, or Proxmox refuses it.
pub fn proxmox(yaml: &str) -> Outcome<String> {
    Ok(file(&ProxmoxBackend.render(&plan(yaml, Target::Proxmox)?)?, "proxmox/main.tf")?.to_owned())
}

/// Why `backend` refuses `yaml` (a spec, valid or not), if it does.
///
/// # Errors
///
/// When the spec doesn't parse.
pub fn refusal(backend: &dyn Backend, yaml: &str) -> Outcome<Option<String>> {
    Ok(backend.refusal(&planned(&parsed(yaml)?, backend.target())))
}

/// The lab of `yaml`, valid or not.
fn parsed(yaml: &str) -> Outcome<Lab> {
    Lab::parse(yaml).map_err(|problems| format!("the spec doesn't parse: {problems:?}").into())
}

/// `lab`, when nothing is wrong with its spec.
fn valid(lab: Lab) -> Outcome<Lab> {
    let problems = lab.problems();
    if problems.is_empty() {
        Ok(lab)
    } else {
        Err(format!("the spec isn't valid: {problems:?}").into())
    }
}

/// The contents of the file among `files` whose path ends with `suffix`.
///
/// # Errors
///
/// When there is none.
pub fn file<'a>(files: &'a [GeneratedFile], suffix: &str) -> Outcome<&'a str> {
    files
        .iter()
        .find(|f| f.path.ends_with(suffix))
        .map(|f| f.contents.as_str())
        .ok_or_else(|| format!("no file ending with {suffix}").into())
}
