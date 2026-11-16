//! The conformance runs' containers: a family's pinned official image, the project and the
//! scripts copied in with `docker cp` (no host path mounted), CPUs capped, removed after.

use std::path::Path;

use super::lab::Family;
use crate::{Res, process};

/// The probe of each kind's state.
pub(super) const PROBE: &str = include_str!("probe.sh");

/// What the container runs: the applies and the probes.
pub(super) const RUN: &str = include_str!("run.sh");

/// What the cloud-init container runs: real cloud-init (the first apply), then the shell library
/// on top (the second apply), with the probes.
pub(super) const RUN_CLOUD_INIT: &str = include_str!("run-cloudinit.sh");

/// What the Ansible container runs: real Ansible applies the generated role (the first apply),
/// then applies it again (the second apply) and must change nothing, with the probes. The probe is
/// the shell renderer's own, so a pass means Ansible reached the state the shell reaches.
pub(super) const RUN_ANSIBLE: &str = include_str!("run-ansible.sh");

/// The family's pinned official image.
fn image(family: Family) -> Res<&'static str> {
    isoloom_spec::os::by_name(family.id)
        .and_then(|os| os.container_image())
        .ok_or_else(|| format!("{} has no pinned container image", family.id).into())
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
    let mut build = process::tool("docker");
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
    let _removed = process::quiet(process::tool("docker").args(["image", "rm", "-f", &tag]));
    said
}

/// Runs `run.sh` in a container of `image`, the project at `/opt/isoloom` and the scripts at
/// `/isoloom-test`; what it printed. The container is removed after, whatever happened.
fn in_container(image: &str, family: Family, dir: &Path, cpus: u16, mode: &str) -> Res<String> {
    let name = format!("isoloom-conformance-{}-{mode}", family.id.replace('.', "-"));
    let _stale = process::quiet(process::tool("docker").args(["rm", "-f", &name]));
    let mut create = process::tool("docker");
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
    let _removed = process::quiet(process::tool("docker").args(["rm", "-f", &name]));
    said
}

/// Copies the project and the scripts into container `name`, then starts it attached.
fn copy_and_start(name: &str, dir: &Path) -> Res<String> {
    for (from, to) in [("project", "/opt/isoloom"), ("test", "/isoloom-test")] {
        let mut cp = process::tool("docker");
        cp.arg("cp").arg(format!("{}/.", dir.join(from).display())).arg(format!("{name}:{to}"));
        process::quiet(&mut cp)?;
    }
    let out = process::tool("docker")
        .args(["start", "--attach", name])
        .output()
        .map_err(|e| format!("docker start couldn't start: {e}"))?;
    let mut said = String::from_utf8_lossy(&out.stdout).into_owned();
    said.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(said)
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use isoloom_test_support::FakeBin;

    use super::super::lab::FAMILIES;
    use super::*;

    /// A stand-in `docker`, each call logged: `start` prints what a run prints (on both
    /// streams); `cp` fails when `fail-cp` is there.
    fn docker() -> FakeBin {
        let bin = FakeBin::named("xtask-conformance").unwrap();
        let body = format!(
            r#"echo "docker $*" >> '{log}'
case "$1" in
  cp) [ ! -f '{dir}/fail-cp' ] || {{ echo "no such container" >&2; exit 1; }} ;;
  start) echo "FIRST ok packages"; echo "END"; echo "apk: warning" >&2 ;;
esac"#,
            log = bin.log().display(),
            dir = bin.dir().display()
        );
        bin.stand_in("docker", &body).unwrap();
        bin
    }

    fn calls(bin: &FakeBin) -> Vec<String> {
        bin.calls().unwrap().iter().map(|c| c.join(" ")).collect()
    }

    #[test]
    fn a_fresh_run_copies_the_project_and_scripts_in_starts_attached_and_removes_the_container() {
        let bin = docker();
        let family = FAMILIES[0];
        let dir = bin.dir().join("run");
        let said = crate::process::testing::with_stand_ins(&bin.path(), || fresh(family, &dir, 2)).unwrap();
        assert_eq!(said, "FIRST ok packages\nEND\napk: warning\n", "both streams, the run's own first");
        let name = "isoloom-conformance-debian-12-apply";
        let image = image(family).unwrap();
        assert_eq!(
            calls(&bin),
            [
                format!("docker rm -f {name}"),
                format!("docker create --name {name} --cpus 2 --entrypoint sh {image} /isoloom-test/run.sh apt apply"),
                format!("docker cp {}/project/. {name}:/opt/isoloom", dir.display()),
                format!("docker cp {}/test/. {name}:/isoloom-test", dir.display()),
                format!("docker start --attach {name}"),
                format!("docker rm -f {name}"),
            ]
        );
    }

    #[test]
    fn a_container_that_failed_on_the_way_is_removed_all_the_same() {
        let bin = docker();
        std::fs::write(bin.dir().join("fail-cp"), "").unwrap();
        let dir = bin.dir().join("run");
        let e = crate::process::testing::with_stand_ins(&bin.path(), || fresh(FAMILIES[0], &dir, 2)).unwrap_err();
        let name = "isoloom-conformance-debian-12-apply";
        assert_eq!(
            e.to_string(),
            format!(
                "`{} cp {}/project/. {name}:/opt/isoloom` failed (exit status: 1): no such container",
                bin.path().join("docker").display(),
                dir.display()
            )
        );
        assert_eq!(calls(&bin).last(), Some(&format!("docker rm -f {name}")));
    }

    #[test]
    fn the_container_form_builds_its_image_capped_then_runs_and_removes_it() {
        let bin = docker();
        let family = FAMILIES[0];
        let dir = bin.dir().join("run");
        crate::process::testing::with_stand_ins(&bin.path(), || built(family, &dir, 2)).unwrap();
        let calls = calls(&bin);
        let project = dir.join("project");
        assert_eq!(
            calls.first(),
            Some(&format!(
                "docker build --quiet --resource cpu-quota=200000 -t isoloom-conformance:debian-12 -f {} {}",
                project.join(".isoloom/docker/build/m/Dockerfile").display(),
                project.display()
            ))
        );
        let name = "isoloom-conformance-debian-12-built";
        assert!(
            calls.contains(&format!(
                "docker create --name {name} --cpus 2 --entrypoint sh isoloom-conformance:debian-12 /isoloom-test/run.sh apt built"
            )),
            "{calls:?}"
        );
        assert_eq!(calls.last().map(String::as_str), Some("docker image rm -f isoloom-conformance:debian-12"));
    }
}
