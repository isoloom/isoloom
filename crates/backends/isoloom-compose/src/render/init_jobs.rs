//! A machine's `init:` jobs: one-shot services run in order once the machine is ready.

use indexmap::IndexMap;
use isoloom_plan::{Environment, Image};
use isoloom_render::container::init_names;
use isoloom_render::steps::PROJECT_DIR;

use super::service::{build_of, environment, platform, readiness};
use super::{configs, secrets};
use crate::file::{Condition, NetworkMode, RO_BIND, Restart, Service};
use crate::layout::{ROOT, file_name};
use crate::machine::Machine;
use crate::names::routes_service;
use crate::{sidecars, storage};

/// One-shot init jobs, in order, each after the previous one. They share the machine's network
/// namespace (same addresses, names and routes) and its volumes, as a VM's steps see its disk.
pub(super) fn add(env: &Environment, m: Machine<'_>, services: &mut IndexMap<String, Service>) {
    let Some(d) = &m.spec.docker else { return };
    let plan = m.plan;
    let image = match &plan.image {
        Some(Image::Container { reference, .. }) => Some(reference.clone()),
        _ => None,
    };
    // A job runs the image the machine builds, when it builds one: as buildable as the
    // machine's, so a pull that leaves built images out (`pull --ignore-buildable`) leaves it
    // out too, instead of asking a registry for a name only this build gives. `pull_policy`
    // wouldn't do: `pull --policy` overrides it. The build is the machine's, so BuildKit runs
    // it once for both.
    let build = match &plan.image {
        Some(Image::Container { build: Some(b), .. }) => Some(build_of(env, m.name, b)),
        _ => None,
    };
    let mut previous: Option<String> = None;
    for (job, script) in init_names(m.name, m.spec).into_iter().zip(&d.init) {
        let mounted = format!("/isoloom/init/{}", file_name(script));
        // A setup job runs as root with what the machine has: its network, volumes, files (value
        // files included) and privileges, plus the project at /opt/isoloom, exactly as a VM's
        // steps have it, so one `setup:` script finds its files at the same path on every target.
        let mut j = Service {
            image: image.clone(),
            build: build.clone(),
            platform: platform(plan),
            user: Some("root".into()),
            privileged: m.spec.privileged,
            cap_add: if plan.is_gateway { vec!["NET_ADMIN".into()] } else { Vec::new() },
            entrypoint: vec!["/bin/sh".into(), mounted.clone()],
            network_mode: Some(NetworkMode::Service(m.name.to_owned())),
            environment: environment(env, plan),
            secrets: secrets::of_machine(env, plan),
            configs: configs::mounts(m.name, plan),
            restart: Some(Restart::No),
            ..Service::default()
        };
        j.volumes.push(format!("{ROOT}/{script}:{mounted}:{RO_BIND}").into());
        j.volumes.push(format!("{ROOT}:{PROJECT_DIR}:{RO_BIND}").into());
        j.volumes.extend(storage::volume_mounts(plan));
        secrets::mount_defaulted(env, m.name, plan, &mut j);
        j.after(m.name, readiness(m.spec));
        if sidecars::needed(env, m) {
            j.after(routes_service(m.name), Condition::Healthy);
        }
        if let Some(p) = &previous {
            j.after(p.clone(), Condition::CompletedSuccessfully);
        }
        services.insert(job.clone(), j);
        previous = Some(job);
    }
}
