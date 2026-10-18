//! The linux-docker host: a pinned Rust container on this machine
//! (`xtask/check-all/linux.Dockerfile`), the repositories mounted at their own paths, each
//! checkout building in its own volumes ([`checkout::docker_volumes`]); and how `--clean` and
//! `--prune-volumes` clear them.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use super::checkout::{self, Volume};
use super::collect::{Collector, Sink};
use super::hosts::repos_arg;
use super::{Repo, Run, Status};
use crate::{Res, dev_link, process, repo};

/// The repositories whose checkouts a run builds: Isoloom always (it runs the leg), then the
/// others asked for, each with its path (as it is on disk: [`super::Dirs`]).
fn checkouts(run: &Run) -> Vec<(Repo, String)> {
    let mut repos = vec![Repo::Isoloom];
    repos.extend(run.repos.iter().copied().filter(|r| *r != Repo::Isoloom));
    repos.into_iter().map(|r| (r, repo::slashed(run.dirs.of(r)))).collect()
}

/// `CARGO_BUILD_JOBS` for the container: this machine's, else 4.
fn jobs() -> String {
    std::env::var("CARGO_BUILD_JOBS").unwrap_or_else(|_| "4".into())
}

/// The container's image, built once per version of its Dockerfile.
fn image(out: &mut Collector) -> Result<String, String> {
    let dockerfile = repo::root().join("xtask/check-all/linux.Dockerfile");
    let text = std::fs::read(&dockerfile).map_err(|e| format!("{}: {e}", dockerfile.display()))?;
    let digest = crate::text::hex(Sha256::digest(&text).get(..6).unwrap_or_default());
    let image = format!("isoloom-check-all-linux:{digest}");
    if process::quiet(Command::new("docker").args(["image", "inspect", &image])).is_ok() {
        return Ok(image);
    }
    println!("  linux-docker: building {image} (once)");
    let context = dockerfile.parent().unwrap_or(Path::new("."));
    let mut build = Command::new("docker");
    build.args(["build", "-t", &image, "-f"]).arg(&dockerfile).arg(context);
    process::stream(&mut build, &mut |l| out.line(l))
        .ok()
        .filter(std::process::ExitStatus::success)
        .map(|_| image)
        .ok_or_else(|| "image build failed (see linux-docker.log)".to_owned())
}

/// The repositories mounted at their own paths, so dev-link's absolute paths hold, and the
/// Isoloom checkouts linked consumers point at.
fn repo_mounts(run: &Run) -> Vec<String> {
    let dirs = &run.dirs;
    let mut paths: Vec<PathBuf> = Vec::new();
    for d in [&dirs.isoloom, &dirs.isothrum, &dirs.launcher].into_iter().filter(|d| d.is_dir()) {
        paths.push(d.clone());
        for config in dev_link::linked_configs(d) {
            let linked = std::fs::read_to_string(d.join(&config)).ok().and_then(|t| dev_link::linked_isoloom(&t));
            paths.extend(linked.map(PathBuf::from));
        }
    }
    paths.dedup();
    paths.iter().flat_map(|p| ["-v".to_owned(), format!("{0}:{0}", repo::slashed(p))]).collect()
}

/// Creates `volume` with its labels when it isn't there yet (Docker keeps an existing one).
fn create(volume: &Volume) -> Res<()> {
    let mut cmd = Command::new("docker");
    cmd.args(["volume", "create", "--label"])
        .arg(format!("{}={}", checkout::LABEL, volume.holds.label()));
    if let Some(path) = &volume.checkout {
        cmd.arg("--label").arg(format!("{}={path}", checkout::CHECKOUT_LABEL));
    }
    process::quiet(cmd.arg(&volume.name))
}

/// Creates this run's volumes ([`checkout::docker_volumes`]); the first that can't be, with why.
fn volumes(run: &Run, out: &mut Collector) -> Result<Vec<Volume>, String> {
    let volumes = checkout::docker_volumes(&checkouts(run));
    for volume in &volumes {
        create(volume).map_err(|e| format!("can't create the volume {}: {e}", volume.name))?;
        if let Some(path) = &volume.checkout {
            out.line(&format!("{} of {path}: volume {}", volume.holds.label(), volume.name));
        }
    }
    Ok(volumes)
}

/// The container running the leg: the repositories at their own paths, this run's volumes, and
/// the downloads and build folders pointed at them.
fn container(run: &Run, image: &str, volumes: &[Volume]) -> Command {
    let mut cmd = Command::new("docker");
    cmd.args(["run", "--rm"]).args(repo_mounts(run));
    for volume in volumes {
        cmd.arg("-v").arg(format!("{}:{}", volume.name, volume.at));
    }
    let cache = checkout::CACHE_ROOT;
    for env in [
        format!("CARGO_HOME={cache}/cargo-home"),
        format!("COREPACK_HOME={cache}/corepack"),
        format!("npm_config_store_dir={cache}/pnpm-store"),
        "COREPACK_ENABLE_DOWNLOAD_PROMPT=0".to_owned(),
        format!("CARGO_TARGET_DIR={}/isoloom", checkout::TARGET_ROOT),
        format!("CARGO_BUILD_JOBS={}", jobs()),
    ] {
        cmd.arg("-e").arg(env);
    }
    let [isoloom, isothrum, launcher] = [&run.dirs.isoloom, &run.dirs.isothrum, &run.dirs.launcher].map(|d| repo::slashed(d));
    cmd.args(["-w", &isoloom, image]);
    cmd.args([
        "cargo",
        "xtask",
        "check-all",
        "--leg",
        "linux-docker",
        "--profile",
        "platform",
        "--repos",
        &repos_arg(run),
    ]);
    cmd.args(["--isoloom", &isoloom, "--isothrum", &isothrum, "--launcher", &launcher]);
    cmd.args(["--target-root", checkout::TARGET_ROOT]);
    cmd
}

/// The linux-docker host's part.
pub(super) fn linux_docker(run: &Run, out: &mut Collector) {
    if process::quiet(Command::new("docker").arg("info")).is_err() {
        return out.host_failed(Status::Unreachable, "Docker is not running");
    }
    let image = match image(out) {
        Ok(image) => image,
        Err(why) => return out.host_failed(Status::Fail, &why),
    };
    let volumes = match volumes(run, out) {
        Ok(volumes) => volumes,
        Err(why) => return out.host_failed(Status::Fail, &why),
    };
    if let Err(e) = process::stream(&mut container(run, &image, &volumes), &mut |l| out.feed(l)) {
        out.host_failed(Status::Fail, &format!("docker couldn't start: {e}"));
    }
}

/// Every volume on this machine, with check-all's labels.
fn listed() -> Res<Vec<checkout::Listed>> {
    let text = process::output(Command::new("docker").args(["volume", "ls", "--format", checkout::VOLUME_FORMAT]))?;
    Ok(checkout::parse_volumes(&text))
}

/// Removes each volume of `names`, one at a time so one in use doesn't keep the others; what
/// was removed, and what couldn't be.
fn remove(names: &[&str]) -> String {
    let (mut removed, mut kept) = (0, Vec::new());
    for name in names {
        match process::quiet(Command::new("docker").args(["volume", "rm", name])) {
            Ok(()) => removed += 1,
            Err(e) => kept.push(format!("{name} ({e})")),
        }
    }
    let note = if kept.is_empty() {
        String::new()
    } else {
        format!("; kept {}", kept.join(", "))
    };
    format!("removed {removed} volume(s){note}")
}

/// `--clean`: every check-all volume (every checkout's, and the shared downloads) and the image.
pub(super) fn clean() -> Res<String> {
    let listed = listed()?;
    let mut said = remove(&checkout::all_volumes(&listed));
    let images = process::output(Command::new("docker").args(["image", "ls", "-q", "isoloom-check-all-linux"]))?;
    let mut ids: Vec<&str> = images.lines().collect();
    ids.sort_unstable();
    ids.dedup();
    if !ids.is_empty() {
        process::quiet(Command::new("docker").args(["image", "rm", "-f"]).args(&ids))?;
        said.push_str(" and the image");
    }
    Ok(said)
}

/// `--prune-volumes`: the build volumes of the checkouts no longer on this machine, and the
/// shared ones from before keys.
pub(super) fn prune() -> Res<String> {
    let listed = listed()?;
    let exists = |path: &str| Path::new(path).is_dir();
    let stale = checkout::stale_volumes(&listed, &exists);
    for name in &stale {
        println!("  pruning {name}");
    }
    Ok(remove(&stale))
}
