//! Generators: turn a spec into the files a target runs. Each generator is a pure function
//! of the spec (no file system access), so `isoloom check` can regenerate in memory and
//! compare with what's committed. Output lives under `.isoloom/<target>/` in the project.

pub mod backends;
pub mod report;
pub mod resolved;

pub use backends::{BACKENDS, backend};
pub use isoloom_plan::Environment;
pub use isoloom_render::backend::{Backend, Files, RenderError};

pub use isoloom_compose::render::files::project_variables as compose_project_variables;
pub use isoloom_compose::{ENVIRONMENT_LABEL, MANAGED_LABEL, StartPlan, exec_runner, leaf_jobs, removable_volumes, start_commands, start_plan};
pub use isoloom_kubernetes::{project_copies, secrets::SECRETS as KUBERNETES_SECRETS};
pub use isoloom_render::{GenerateError, GeneratedFile, OUTPUT_DIR, constraints, machine_setup, provision_script, router_name};
pub use isoloom_spec::validate::docker_cidrs;

use isoloom_compose::appliances;
use isoloom_plan::checks;
use isoloom_render::{address, router, start_order};
use isoloom_spec::{groups, images, instance, model, targets, validate};
use isoloom_vagrant::vagrant;

/// The network appliances' management network (see `docker.appliance`).
pub const APPLIANCE_MGMT_CIDR: &str = appliances::MGMT_CIDR;
/// Whether an appliance kind is a switch (it can be a LAN's `switch`).
pub fn is_switch_appliance(kind: crate::model::Appliance) -> bool {
    appliances::is_switch(kind)
}

use std::net::Ipv4Addr;

use crate::model::{Spec, Target};
use crate::targets::effective;

/// Targets that have a generator today.
pub const GENERATED_TARGETS: &[Target] = &[
    Target::Docker,
    Target::Hosted,
    Target::DockerVm,
    Target::CloudDocker,
    Target::Kubernetes,
    Target::Hybrid,
    Target::Vagrant,
    Target::Proxmox,
    Target::Esxi,
    Target::CloudVm,
    Target::External,
    Target::CloudServices,
];

/// The files for one target.
pub fn generate(spec: &Spec, target: Target) -> Result<Vec<GeneratedFile>, GenerateError> {
    let mut files = target_files(spec, target, None)?;
    // The resolved snapshot goes with every target (the same file each time).
    files.push(resolved::file(spec));
    Ok(files)
}

/// The files for one target as instance `n` of the spec (see [`crate::instance`]): names
/// suffixed, Docker blocks and published ports moved, everything under `.isoloom-<n>/`.
pub fn generate_instance(spec: &Spec, target: Target, n: u8) -> Result<Vec<GeneratedFile>, GenerateError> {
    let applied = crate::instance::apply(spec, n).map_err(|what| GenerateError::Unsupported { target, what })?;
    let mut files = target_files(&applied, target, Some(n))?;
    files.push(resolved::file_with(&applied, Some(n)));
    Ok(files
        .into_iter()
        .map(|f| {
            let (path, contents) = crate::instance::relocate(&f.path, &f.contents, n);
            GeneratedFile { path, contents }
        })
        .collect())
}

/// A target's own files, without the snapshot (which asks every backend whether it would
/// refuse the spec, so it can't be part of what a backend renders): the plan for the target,
/// rendered by its backend.
fn target_files(spec: &Spec, target: Target, instance: Option<u8>) -> Result<Vec<GeneratedFile>, GenerateError> {
    if !effective(spec).contains(&target) {
        return Err(GenerateError::NotPossible(target));
    }
    let backend = backend(target).ok_or(GenerateError::NoGenerator(target))?;
    let env = Environment::build(spec, target, instance);
    let mut files = backend.render(&env)?;
    if let Some(manifest) = backend.run_manifest(&env)? {
        files.push(run_manifest(spec, instance, target, manifest)?);
    }
    Ok(files)
}

/// A backend's run manifest as the file it is written to (`.isoloom/<target>/run.json`), its
/// message filled from the resolved snapshot, checked across its fields.
fn run_manifest(spec: &Spec, instance: Option<u8>, target: Target, mut manifest: isoloom_manifest::Manifest) -> Result<GeneratedFile, GenerateError> {
    let refused = |e: isoloom_manifest::ManifestError| GenerateError::Unsupported {
        target,
        what: format!("the run manifest: {e}"),
    };
    manifest.lab.message = resolved::manifest_message(spec, instance, target);
    manifest.validate().map_err(refused)?;
    Ok(GeneratedFile {
        path: format!("{OUTPUT_DIR}/{}/run.json", target.id()),
        contents: isoloom_manifest::to_json(&manifest).map_err(refused)?,
    })
}

/// Every target the spec allows and that has a generator: the files, and why the others
/// weren't generated.
pub fn generate_all(spec: &Spec) -> (Vec<GeneratedFile>, Vec<GenerateError>) {
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for target in effective(spec) {
        if !GENERATED_TARGETS.contains(&target) {
            skipped.push(GenerateError::NoGenerator(target));
            continue;
        }
        match generate(spec, target) {
            Ok(f) => {
                // Outputs share files (docker-vm runs the Compose file): each once.
                for file in f {
                    if !files.iter().any(|x: &GeneratedFile| x.path == file.path) {
                        files.push(file);
                    }
                }
            }
            Err(e) => skipped.push(e),
        }
    }
    (files, skipped)
}

/// Why `target`'s generator would refuse this spec, if it would: the thing it doesn't support
/// yet (a Windows machine on Proxmox, environment-level provisioning on Docker, ...). `None`
/// when `generate` produces the target. Lets `isoloom targets` tell the truth: a target can be
/// possible by its machines' editions and still not be generated.
///
/// Safe on a spec that hasn't been validated: an invalid spec is refused by every target, with
/// its first problem as the reason, instead of reaching a generator that assumes validity.
pub fn refusal(spec: &Spec, target: Target) -> Option<String> {
    if let Some(problem) = first_problem(spec) {
        return Some(problem);
    }
    if !effective(spec).contains(&target) {
        return None;
    }
    backend(target)?.refusal(&Environment::build(spec, target, None))
}

/// A cloud refused by a multi-cloud target: the cloud's folder under the target's, and why it
/// can't run the environment.
pub type CloudRefusal = (&'static str, String);

/// The clouds of a target that writes one module per cloud (`cloud-vm`), when the target is
/// generated: the clouds whose module is written, and each refused cloud with its reason. A
/// cloud refusing drops only its module, so the target is generated as long as one cloud
/// accepts the environment. `None` for every other target, and for a `cloud-vm` that isn't
/// generated (its refusal says why).
pub fn cloud_refusals(spec: &Spec, target: Target) -> Option<(Vec<&'static str>, Vec<CloudRefusal>)> {
    if target != Target::CloudVm || !effective(spec).contains(&target) || refusal(spec, target).is_some() {
        return None;
    }
    let env = Environment::build(spec, target, None);
    let (mut generated, mut refused) = (Vec::new(), Vec::new());
    for (cloud, why) in isoloom_terraform::cloud_vm::cloud_refusals(&env) {
        match why {
            None => generated.push(cloud),
            Some(why) => refused.push((cloud, why)),
        }
    }
    Some((generated, refused))
}

/// [`refusal`], for a target run with a given provider: why `target` can't run this spec when
/// its tool uses `provider`, if it can't. `provider` is the name the target's tool knows it by,
/// as `defaults.yml` sets it: for Vagrant, `vagrant up --provider` (`virtualbox`, `libvirt`,
/// `qemu`, ...). `None` is the target whatever the provider, which is [`refusal`].
///
/// Today `vagrant` refuses more on some providers: a machine requiring a TPM (`vm.tpm: true`)
/// anywhere but `virtualbox`, `libvirt` and `hyperv`; on `qemu`, a VM is on one network (QEMU
/// gives it one); on `utm`, a VM is on none (vagrant_utm has no private networks); on `hyperv`,
/// a machine whose OS has no known Hyper-V box needs `vm.image.hyperv`; `qemu`, `hyperv` and
/// `libvirt` have no bridged networks yet; a machine booting its own way (`vm.boot`) runs on
/// `libvirt` (on `qemu` with UEFI and `firmware_format` only). `isoloom run vagrant` asks it
/// for the provider in the defaults before starting; a tool offering a provider can ask before
/// it does.
pub fn refusal_on(spec: &Spec, target: Target, provider: Option<&str>) -> Option<String> {
    refusal(spec, target).or_else(|| match (target, provider) {
        (Target::Vagrant, Some(p)) => vagrant::provider_refusal(spec, p).or_else(|| vagrant::disk_refusal(spec, p)),
        _ => None,
    })
}

/// The spec's first validation problem, as `refusal` reports it.
fn first_problem(spec: &Spec) -> Option<String> {
    crate::validate::validate(spec).first().map(|p| format!("the spec is invalid: {p}"))
}

/// The spec as the Docker target lays it out: every network on its Docker block.
fn on_docker(spec: &Spec) -> Spec {
    let mut s = spec.clone();
    for (name, c) in docker_cidrs(spec) {
        // `docker_cidrs` names the spec's own networks.
        if let Some(n) = s.networks.get_mut(&name) {
            n.cidr = format!("{}/{}", Ipv4Addr::from(c.base), c.len);
        }
    }
    s
}
