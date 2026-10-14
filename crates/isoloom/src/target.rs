//! The target a command works on: named by its id, or found from what `run` recorded.

use std::path::Path;

use isoloom_runtime::registry::{self, Entry};
use isoloom_spec::Target;

use isoloom_runtime::events::ErrorCode;

use crate::Res;
use crate::failure::fail;

/// The target with this id, or an error naming it.
pub fn by_id(id: &str) -> Res<Target> {
    Target::ALL
        .into_iter()
        .find(|t| t.id() == id)
        .ok_or_else(|| fail(ErrorCode::TargetUnknown, format!("unknown target `{id}`")))
}

/// The folder of a local VM target's output (vagrant, docker-vm or hybrid).
pub fn vm_folder(t: Target) -> &'static str {
    match t {
        Target::Vagrant => "vagrant",
        Target::DockerVm => "docker-vm",
        _ => "hybrid",
    }
}

/// The cloud `cloud-docker` runs on, which it can't guess.
pub fn docker_cloud(cloud: Option<&str>) -> Res<&str> {
    cloud.ok_or_else(|| fail(ErrorCode::Usage, "`cloud-docker` needs --cloud (aws, azure, gcp, digitalocean, linode, oci)"))
}

/// The target of a project to talk to: the one named; else what `run` recorded for the
/// project (when there is exactly one); else the spec's single possible target.
pub fn pick(dir: &Path, target: Option<&str>, instance: Option<u8>, spec: &isoloom_spec::Spec) -> Res<(Target, Option<String>)> {
    let reg = registry::load().unwrap_or_default();
    let recorded: Vec<&Entry> = reg.for_dir(dir).into_iter().filter(|e| e.instance == instance).collect();
    if let Some(id) = target {
        let t = by_id(id)?;
        let cloud = recorded.iter().find(|e| e.target == t).and_then(|e| e.cloud.clone());
        return Ok((t, cloud));
    }
    match recorded.as_slice() {
        [one] => Ok((one.target, one.cloud.clone())),
        [] => match isoloom_spec::effective(spec).as_slice() {
            [one] => Ok((*one, None)),
            many => Err(fail(
                ErrorCode::NotRunning,
                format!(
                    "nothing recorded as running here{} (`isoloom status`); pass the target: {}",
                    instance.map(|n| format!(" as instance {n}")).unwrap_or_default(),
                    many.iter().map(|t| format!("--target {}", t.id())).collect::<Vec<_>>().join(", ")
                ),
            )),
        },
        several => Err(fail(
            ErrorCode::TargetUnavailable,
            format!(
                "several targets are up here ({}); pass one with --target",
                several.iter().map(|e| e.target.id()).collect::<Vec<_>>().join(", ")
            ),
        )),
    }
}
