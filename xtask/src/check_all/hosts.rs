//! Where each host's part runs: here (mac), over SSH (linux and windows, in [`remote`]), or in a
//! container on this machine (linux-docker); and how `--clean` clears each.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use super::collect::{Collector, Sink};
use super::{Dirs, Host, Profile, Run, Status, gates, remote};
use crate::{dev_link, process, repo};

/// The Docker volume holding the container's build folders and caches.
const DOCKER_VOLUME: &str = "isoloom-check-all-linux-docker";

/// Runs `host`'s part, its results into `out`.
pub fn run_host(host: Host, run: &Run, out: &mut Collector) {
    match host {
        Host::Mac => mac(run, out),
        Host::Linux => remote::linux(run, out),
        Host::Windows => remote::windows(run, out),
        Host::LinuxDocker => linux_docker(run, out),
    }
}

fn mac(run: &Run, out: &mut Collector) {
    if std::env::consts::OS != "macos" {
        return out.host_failed(Status::Skip, "not a Mac");
    }
    gates::leg(Host::Mac, Profile::Full, &run.dirs, None).run(&run.repos, out);
}

/// The repositories as a `--repos` value.
pub fn repos_arg(run: &Run) -> String {
    run.repos.iter().map(|r| r.id()).collect::<Vec<_>>().join(",")
}

/// `CARGO_BUILD_JOBS` for the other hosts: this one's, else 4.
pub fn jobs() -> String {
    std::env::var("CARGO_BUILD_JOBS").unwrap_or_else(|_| "4".into())
}

/// The container's image, built once per version of its Dockerfile.
fn docker_image(out: &mut Collector) -> Result<String, String> {
    let dockerfile = repo::root().join("xtask/check-all/linux.Dockerfile");
    let text = std::fs::read(&dockerfile).map_err(|e| format!("{}: {e}", dockerfile.display()))?;
    let digest = crate::text::hex(&Sha256::digest(&text)[..6]);
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
        .ok_or_else(|| "image build failed (see linux-docker.log)".to_string())
}

/// The repositories mounted at their own paths, so dev-link's absolute paths hold, and the
/// Isoloom checkouts linked consumers point at.
fn mounts(dirs: &Dirs) -> Vec<String> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for d in [&dirs.isoloom, &dirs.isothrum, &dirs.launcher].into_iter().filter(|d| d.is_dir()) {
        paths.push(d.clone());
        for config in dev_link::linked_configs(d) {
            let linked = std::fs::read_to_string(d.join(&config)).ok().and_then(|t| dev_link::linked_isoloom(&t));
            paths.extend(linked.map(PathBuf::from));
        }
    }
    paths.dedup();
    paths.iter().flat_map(|p| ["-v".to_string(), format!("{0}:{0}", repo::slashed(p))]).collect()
}

fn linux_docker(run: &Run, out: &mut Collector) {
    if process::quiet(Command::new("docker").arg("info")).is_err() {
        return out.host_failed(Status::Unreachable, "Docker is not running");
    }
    let image = match docker_image(out) {
        Ok(image) => image,
        Err(why) => return out.host_failed(Status::Fail, &why),
    };
    let (isoloom, isothrum, launcher) = (
        repo::slashed(&run.dirs.isoloom),
        repo::slashed(&run.dirs.isothrum),
        repo::slashed(&run.dirs.launcher),
    );
    let mut cmd = Command::new("docker");
    cmd.args(["run", "--rm"]).args(mounts(&run.dirs));
    // The launcher's node_modules shadowed by a volume: the host's has macOS binaries.
    cmd.args([
        "-v",
        &format!("{DOCKER_VOLUME}:/check-all"),
        "-v",
        &format!("{DOCKER_VOLUME}-node-modules:{launcher}/node_modules"),
    ]);
    for env in [
        "CARGO_HOME=/check-all/cargo-home",
        "COREPACK_HOME=/check-all/corepack",
        "npm_config_store_dir=/check-all/pnpm-store",
        "COREPACK_ENABLE_DOWNLOAD_PROMPT=0",
        "CARGO_TARGET_DIR=/check-all/target/isoloom",
    ] {
        cmd.args(["-e", env]);
    }
    cmd.args(["-e", &format!("CARGO_BUILD_JOBS={}", jobs()), "-w", &isoloom, &image]);
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
    cmd.args([
        "--isoloom",
        &isoloom,
        "--isothrum",
        &isothrum,
        "--launcher",
        &launcher,
        "--target-root",
        "/check-all/target",
    ]);
    if let Err(e) = process::stream(&mut cmd, &mut |l| out.feed(l)) {
        out.host_failed(Status::Fail, &format!("docker couldn't start: {e}"));
    }
}

/// Removes the hosts' build folders and copies.
pub fn clean(hosts: &[Host], dirs: &Dirs) {
    for host in hosts {
        let done = match host {
            Host::Linux => remote::clean_linux(),
            Host::Windows => remote::clean_windows(),
            Host::LinuxDocker => clean_docker(),
            Host::Mac => Ok(format!("the repositories' own build folders stay (cargo clean in {})", dirs.isoloom.display())),
        };
        match done {
            Ok(what) => println!("{}: {what}", host.id()),
            Err(e) => println!("{}: {e}", host.id()),
        }
    }
}

fn clean_docker() -> crate::Res<String> {
    let volumes = [DOCKER_VOLUME.to_string(), format!("{DOCKER_VOLUME}-node-modules")];
    process::quiet(Command::new("docker").args(["volume", "rm"]).args(&volumes))?;
    let images = process::output(Command::new("docker").args(["image", "ls", "-q", "isoloom-check-all-linux"]))?;
    let mut ids: Vec<&str> = images.lines().collect();
    ids.sort_unstable();
    ids.dedup();
    if !ids.is_empty() {
        process::quiet(Command::new("docker").args(["image", "rm", "-f"]).args(&ids))?;
    }
    Ok("removed the volumes and the image".into())
}
