//! Generators: turn a spec into the files a target runs. Each generator is a pure function
//! of the spec (no file system access), so `isoloom check` can regenerate in memory and
//! compare with what's committed. Output lives under `.isoloom/<target>/` in the project.

pub mod resolved;

pub use isoloom_compose::{ENVIRONMENT_LABEL, MANAGED_LABEL, StartPlan, exec_runner, leaf_jobs, start_commands, start_plan};
pub use isoloom_kubernetes::project_copies;
pub use isoloom_render::{GenerateError, GeneratedFile, OUTPUT_DIR, constraints, machine_setup, provision_script, router_name};
pub use isoloom_spec::validate::docker_cidrs;

use isoloom_compose::{appliances, docker};
use isoloom_external::external;
use isoloom_kubernetes::kubernetes;
use isoloom_plan::checks;
use isoloom_render::{address, project, router, start_order};
use isoloom_spec::{groups, images, instance, model, targets, validate};
use isoloom_terraform::{cloud_docker, cloud_services, cloud_vm, proxmox};
use isoloom_vagrant::{docker_vm, hybrid, vagrant};

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
    Target::CloudVm,
    Target::External,
    Target::CloudServices,
];

/// The files for one target.
pub fn generate(spec: &Spec, target: Target) -> Result<Vec<GeneratedFile>, GenerateError> {
    let mut files = target_files(spec, target)?;
    // The resolved snapshot goes with every target (the same file each time).
    files.push(resolved::file(spec));
    Ok(files)
}

/// The files for one target as instance `n` of the spec (see [`crate::instance`]): names
/// suffixed, Docker blocks and published ports moved, everything under `.isoloom-<n>/`.
pub fn generate_instance(spec: &Spec, target: Target, n: u8) -> Result<Vec<GeneratedFile>, GenerateError> {
    let applied = crate::instance::apply(spec, n).map_err(|what| GenerateError::Unsupported { target, what })?;
    let mut files = target_files(&applied, target)?;
    files.push(resolved::file_with(&applied, Some(n)));
    Ok(files
        .into_iter()
        .map(|f| {
            let (path, contents) = crate::instance::relocate(&f.path, &f.contents, n);
            GeneratedFile { path, contents }
        })
        .collect())
}

/// A target's own files, without the snapshot (which asks every generator whether it would
/// refuse the spec, so it can't be part of what a generator produces).
fn target_files(spec: &Spec, target: Target) -> Result<Vec<GeneratedFile>, GenerateError> {
    if !effective(spec).contains(&target) {
        return Err(GenerateError::NotPossible(target));
    }
    // Network appliances run on Docker Compose only (their wiring is Docker's).
    if !matches!(target, Target::Docker | Target::Hosted | Target::CloudDocker | Target::DockerVm)
        && let Some((name, _, _)) = appliances::appliances(spec).first()
    {
        return Err(GenerateError::Unsupported {
            target,
            what: format!("machine `{name}` is a network appliance (`docker.appliance`), which runs on the Docker targets"),
        });
    }
    let mut files = target_files_raw(spec, target)?;
    for f in files.iter_mut().filter(|f| f.path.ends_with(".tf")) {
        project::terraform(&mut f.contents);
    }
    constraints::rewrite_images(spec, &mut files);
    constraints::terraform_prelude(spec, &mut files);
    Ok(files)
}

fn target_files_raw(spec: &Spec, target: Target) -> Result<Vec<GeneratedFile>, GenerateError> {
    match target {
        // A hosting service runs the same Compose file.
        Target::Docker | Target::Hosted => docker::generate(&on_docker(spec), spec),
        // Docker on one VM runs the Compose file: both are generated.
        Target::CloudDocker => {
            let mut files = docker::generate(&on_docker(spec), spec)?;
            files.extend(cloud_docker::generate(spec)?);
            Ok(files)
        }
        Target::DockerVm => {
            let mut files = docker::generate(&on_docker(spec), spec)?;
            files.extend(docker_vm::generate(spec)?);
            Ok(files)
        }
        Target::Kubernetes => kubernetes::generate(spec),
        Target::Hybrid => hybrid::generate(spec),
        Target::Vagrant => vagrant::generate(spec),
        Target::Proxmox => proxmox::generate(spec),
        Target::CloudVm => cloud_vm::generate(spec),
        Target::External => external::generate(spec),
        Target::CloudServices => cloud_services::generate(spec),
    }
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
    match target_files(spec, target) {
        Err(GenerateError::Unsupported { what, .. }) => Some(what),
        _ => None,
    }
}

/// [`refusal`], for a target run with a given provider: why `target` can't run this spec when
/// its tool uses `provider`, if it can't. `provider` is the name the target's tool knows it by,
/// as `defaults.yml` sets it: for Vagrant, `vagrant up --provider` (`virtualbox`, `libvirt`,
/// `qemu`, ...). `None` is the target whatever the provider, which is [`refusal`].
///
/// Today only `vagrant` on `qemu` refuses more: without root, QEMU links exactly two VMs per
/// network, and each VM is on one network. `isoloom run vagrant` asks it for the provider in
/// the defaults before starting; a tool offering a provider can ask before it does.
pub fn refusal_on(spec: &Spec, target: Target, provider: Option<&str>) -> Option<String> {
    refusal(spec, target).or_else(|| match (target, provider) {
        (Target::Vagrant, Some("qemu")) => vagrant::qemu_refusal(spec),
        _ => None,
    })
}

/// Why this environment's VMs can't all have their networks under Vagrant's QEMU provider, if
/// they can't. Only the QEMU part of the answer: it doesn't say whether Vagrant can run the spec
/// at all.
#[deprecated(
    since = "0.11.0",
    note = "use `refusal_on(spec, Target::Vagrant, Some(\"qemu\"))`, which also says why Vagrant can't run the spec at all"
)]
pub fn qemu_refusal(spec: &Spec) -> Option<String> {
    vagrant::qemu_refusal(spec)
}

/// The spec's first validation problem, as `refusal` reports it.
fn first_problem(spec: &Spec) -> Option<String> {
    crate::validate::validate(spec).first().map(|p| format!("the spec is invalid: {p}"))
}

/// The spec as the Docker target lays it out: every network on its Docker block.
fn on_docker(spec: &Spec) -> Spec {
    let mut s = spec.clone();
    for (name, c) in docker_cidrs(spec) {
        let n = s.networks.get_mut(&name).expect("same networks");
        n.cidr = format!("{}/{}", Ipv4Addr::from(c.base), c.len);
    }
    s
}
