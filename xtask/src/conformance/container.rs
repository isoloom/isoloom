//! The conformance runs' containers: a family's pinned official image, the project and the
//! scripts copied in with `docker cp` (no host path mounted), CPUs capped, removed after.

use std::path::Path;
use std::process::Command;

use super::lab::Family;
use crate::{Res, process};

/// The probe of each kind's state.
pub(super) const PROBE: &str = include_str!("probe.sh");

/// What the container runs: the applies and the probes.
pub(super) const RUN: &str = include_str!("run.sh");

/// What the cloud-init container runs: real cloud-init (the first apply), then the shell library
/// on top (the second apply), with the probes.
pub(super) const RUN_CLOUD_INIT: &str = include_str!("run-cloudinit.sh");

/// The family's pinned official image.
fn image(family: Family) -> Res<&'static str> {
    isoloom_spec::os::container_image(family.id).ok_or_else(|| format!("{} has no pinned container image", family.id).into())
}

/// A fresh container of the family's image: the first apply, the probe, the second apply, the
/// probe again; what it printed.
pub(super) fn fresh(family: Family, dir: &Path, cpus: u16) -> Res<String> {
    in_container(image(family)?, family, dir, cpus, "apply")
}

/// The image the container form builds, then the probe, the VM's script applied in it and the
/// probe again; what it printed. The image is removed after.
pub(super) fn built(family: Family, dir: &Path, cpus: u16) -> Res<String> {
    let tag = format!("isoloom-conformance:{}", family.id);
    let project = dir.join("project");
    let mut build = Command::new("docker");
    build
        .args([
            "build",
            "--quiet",
            "--resource",
            &format!("cpu-quota={}", u32::from(cpus) * 100_000),
            "-t",
            &tag,
            "-f",
        ])
        .arg(project.join(".isoloom/docker/build/m/Dockerfile"))
        .arg(&project);
    process::quiet(&mut build)?;
    let said = in_container(&tag, family, dir, cpus, "built");
    let _removed = process::quiet(Command::new("docker").args(["image", "rm", "-f", &tag]));
    said
}

/// Runs `run.sh` in a container of `image`, the project at `/opt/isoloom` and the scripts at
/// `/isoloom-test`; what it printed. The container is removed after, whatever happened.
fn in_container(image: &str, family: Family, dir: &Path, cpus: u16, mode: &str) -> Res<String> {
    let name = format!("isoloom-conformance-{}-{mode}", family.id.replace('.', "-"));
    let _stale = process::quiet(Command::new("docker").args(["rm", "-f", &name]));
    let mut create = Command::new("docker");
    create.args([
        "create",
        "--name",
        &name,
        "--cpus",
        &cpus.to_string(),
        "--entrypoint",
        "sh",
        image,
        "/isoloom-test/run.sh",
        family.kind,
        mode,
    ]);
    process::quiet(&mut create)?;
    let said = copy_and_start(&name, dir);
    let _removed = process::quiet(Command::new("docker").args(["rm", "-f", &name]));
    said
}

/// Copies the project and the scripts into container `name`, then starts it attached.
fn copy_and_start(name: &str, dir: &Path) -> Res<String> {
    for (from, to) in [("project", "/opt/isoloom"), ("test", "/isoloom-test")] {
        let mut cp = Command::new("docker");
        cp.arg("cp").arg(format!("{}/.", dir.join(from).display())).arg(format!("{name}:{to}"));
        process::quiet(&mut cp)?;
    }
    let out = Command::new("docker")
        .args(["start", "--attach", name])
        .output()
        .map_err(|e| format!("docker start couldn't start: {e}"))?;
    let mut said = String::from_utf8_lossy(&out.stdout).into_owned();
    said.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(said)
}
