//! The `docker` target: one Compose file at `.isoloom/docker/compose.yml`.
//!
//! - Each network becomes a Compose network with its subnet (Docker at `.1`, or at the last
//!   address when a machine is the network's gateway); `internet: false` makes it `internal`
//!   when nothing routes between networks.
//! - A machine that is a network's gateway gets forwarding and `NET_ADMIN`; the machines
//!   behind it route everything through it (in a sidecar, once it answers).
//! - Each machine becomes a service with its fixed address on every network; services find
//!   each other by name (Compose DNS).
//! - Machines with services get a healthcheck (TCP probe), so `depends_on` waits for them
//!   to answer and `docker compose up --wait` means "everything answers". The probe is
//!   Isoloom's own static busybox (a one-shot `isoloom-probe-<arch>` copies it into a volume),
//!   so it needs nothing from the image: distroless and `scratch` machines work too.
//! - `init:` scripts run once, in one-shot containers of the machine's image on its networks,
//!   after it answers; machines depending on it wait for them to finish.
//! - Inputs become environment variables read from the shell (`${NAME:-}`), only on the
//!   machines that list them.
//! - Checks run in a `check` profile, one runner per position (`isoloom-check` where a user
//!   stands, `isoloom-check-<machine>` for the others), each a sh script in `checks/`.
//!
//! - A service's `publish` port is published on the host's loopback (127.0.0.1) only, on an
//!   ephemeral host port Docker picks, so two labs publishing the same port never collide; the
//!   runner reads the real port back from `docker compose ps`. ISOLOOM_PUBLISH_ADDRESS overrides
//!   the address and ISOLOOM_PUBLISH_FIXED pins the host port to `publish` (both set inside the
//!   docker-vm/cloud VMs, where the lab is alone and that fixed port is forwarded from loopback).
//! - `volumes:` become named volumes: they survive re-creating a container, and go with
//!   `docker compose down -v`.
//!
//! Nothing is published on the host: the environment is reached from its own networks.

use std::fmt::Write as _;

use serde_yaml_ng::{Mapping, Value};

use crate::model::{Arch, Machine, Spec};

pub(crate) const DIR: &str = "docker";
/// From `.isoloom/docker/` back to the project folder.
pub(crate) const ROOT: &str = "../..";
/// On everything an environment creates on Docker: `isoloom gc` collects only what has it.
pub const MANAGED_LABEL: &str = "isoloom.managed";
/// The environment a Docker resource belongs to.
pub const ENVIRONMENT_LABEL: &str = "isoloom.environment";

/// The check runner: a small image with `sh`, `curl` and busybox `nc`.
pub const CHECK_IMAGE: &str = "curlimages/curl:8.11.1";

/// A static busybox Isoloom brings into every machine with services, so its probe needs
/// nothing from the machine's image (distroless and `scratch` images have no shell).
pub const PROBE_IMAGE: &str = "busybox:1.37.0-musl";
/// Where a machine sees that busybox (read-only).
pub const PROBE_DIR: &str = "/.isoloom-probe";

/// The one-shot service (Compose) that copies the probe for machines of this architecture.
pub fn probe_service(arch: Arch) -> String {
    format!("isoloom-probe-{}", arch.id())
}

/// A TCP probe for each service port but the dormant ones, run by Isoloom's own busybox (see
/// [`PROBE_IMAGE`]).
pub fn probe(m: &Machine) -> String {
    m.ready_ports()
        .iter()
        .map(|port| format!("{PROBE_DIR}/busybox nc -z -w 2 127.0.0.1 {port}"))
        .collect::<Vec<_>>()
        .join(" && ")
}

/// What a machine that waits for `m` waits for: healthy (its probe passes) when it has a port
/// to probe, else started.
pub fn ready_condition(m: &Machine) -> &'static str {
    if m.ready_ports().is_empty() { "service_started" } else { "service_healthy" }
}

/// `path` (from the project folder) as seen from the folder `from` (also from the project).
pub fn relative_to(from: &str, path: &str) -> String {
    let clean = |p: &str| -> Vec<String> { p.split('/').filter(|c| !c.is_empty() && *c != ".").map(String::from).collect() };
    let (f, p) = (clean(from), clean(path));
    let common = f.iter().zip(&p).take_while(|(a, b)| a == b).count();
    let mut out: Vec<String> = std::iter::repeat_n("..".to_string(), f.len() - common).collect();
    out.extend(p[common..].iter().cloned());
    out.join("/")
}

/// The one-shot `init:` jobs no running service waits for: those of machines nothing depends
/// on. Compose's `up --wait` fails when such a job exits, even with 0, so starters bring the
/// rest up with `--wait` first, then run these attached, in order (see [`start_commands`]).
pub fn leaf_jobs(spec: &Spec) -> Vec<String> {
    spec.machines
        .iter()
        .filter(|(name, m)| m.docker.is_some() && !spec.machines.values().any(|o| o.docker.is_some() && o.depends_on.iter().any(|d| d == *name)))
        .flat_map(|(name, m)| init_names(name, m))
        .collect()
}

/// How to start a Compose file: `wait` for everything but `jobs` (`up -d --wait`), then run
/// each of `jobs` attached, in order, failing on its exit code. Read from the Compose file, so
/// an embedder holding only `.isoloom/docker/compose.yml` gets the same plan as Isoloom.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StartPlan {
    /// Services to bring up with `docker compose up -d --wait` (empty: all of them).
    pub wait: Vec<String>,
    /// One-shot jobs nothing running waits for, to run after `wait`, in order.
    pub jobs: Vec<String>,
}

/// The [`StartPlan`] of a Compose file: one-shots (`restart: "no"`) outside profiles that no
/// other started service depends on are jobs; when there are none, `wait` is empty (start all).
pub fn start_plan(compose_yaml: &str) -> Result<StartPlan, String> {
    let doc: Value = serde_yaml_ng::from_str(compose_yaml).map_err(|e| e.to_string())?;
    let Some(Value::Mapping(services)) = doc.get("services") else {
        return Ok(StartPlan::default());
    };
    let started: Vec<(&str, &Mapping)> = services
        .iter()
        .filter_map(|(k, v)| Some((k.as_str()?, v.as_mapping()?)))
        .filter(|(_, v)| v.get("profiles").is_none())
        .collect();
    let deps_of = |v: &Mapping| -> Vec<String> {
        match v.get("depends_on") {
            Some(Value::Mapping(d)) => d.keys().filter_map(|k| k.as_str().map(String::from)).collect(),
            Some(Value::Sequence(d)) => d.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            _ => Vec::new(),
        }
    };
    let one_shot = |v: &Mapping| v.get("restart").and_then(Value::as_str) == Some("no");
    // What the long-running services need, through chains of one-shots: `--wait` accepts those
    // exiting. The other one-shots are the jobs.
    let mut needed: Vec<String> = Vec::new();
    let mut todo: Vec<String> = started.iter().filter(|(_, v)| !one_shot(v)).flat_map(|(_, v)| deps_of(v)).collect();
    while let Some(n) = todo.pop() {
        if needed.contains(&n) {
            continue;
        }
        if let Some((_, v)) = started.iter().find(|(k, _)| *k == n) {
            todo.extend(deps_of(v));
        }
        needed.push(n);
    }
    let jobs: Vec<String> = started
        .iter()
        .filter(|(n, v)| one_shot(v) && !needed.iter().any(|x| x == n))
        .map(|(n, _)| n.to_string())
        .collect();
    if jobs.is_empty() {
        return Ok(StartPlan::default());
    }
    let wait = started.iter().map(|(n, _)| n.to_string()).filter(|n| !jobs.contains(n)).collect();
    Ok(StartPlan { wait, jobs })
}

/// The shell command that starts a Compose project with `compose` (e.g. `docker compose -f x`):
/// plain `up -d --build --wait` when no job is left over, else the two steps of [`StartPlan`].
/// `jobs` are the leaf jobs ([`leaf_jobs`]); the services to wait for are listed at run time.
pub fn start_commands(compose: &str, jobs: &[String], wait_timeout: Option<u32>) -> String {
    let timeout = wait_timeout.map(|t| format!(" --wait-timeout {t}")).unwrap_or_default();
    if jobs.is_empty() {
        return format!("{compose} up -d --build --wait{timeout}");
    }
    let exclude = jobs.iter().fold(String::new(), |mut out, j| {
        let _ = write!(out, " -e {j}");
        out
    });
    let mut cmd = format!("{compose} up -d --build --wait{timeout} $({compose} config --services | grep -vx{exclude})");
    for j in jobs {
        let _ = write!(cmd, " && {compose} up --no-deps --exit-code-from {j} {j}");
    }
    cmd
}

/// The runner of a machine's `exec` checks, in `.isoloom/docker/checks/`: piped into the
/// machine (`docker compose exec -T <machine> sh -s < exec-<machine>.sh`).
pub fn exec_runner(machine: &str) -> String {
    format!("exec-{machine}.sh")
}

/// Init job service names for a machine.
pub(crate) fn init_names(name: &str, m: &Machine) -> Vec<String> {
    let n = m.docker.as_ref().map_or(0, |d| d.init.len());
    (1..=n).map(|i| format!("{name}-init-{i}")).collect()
}

/// The last component of a `/`-separated path (a script's name in the container).
pub fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The `shell` tool's image: a toolbox with tcpdump, nmap, curl, dig, netcat and more.
pub const TOOLBOX_IMAGE: &str = "nicolaka/netshoot";

/// Stands in for an access machine the runner supplies, so checks run from its side.
pub(crate) const STAND_IN: &str = "isoloom-access";
/// A small image with busybox `ip`, for the router, route sidecars and the stand-in.
pub const UTILITY_IMAGE: &str = "alpine:3.20";

/// The sidecar image of a machine on a network with `tc`: one with `tc` (the utility image has
/// none, and an offline machine couldn't install it).
pub(crate) const TC_IMAGE: &str = "nicolaka/netshoot:v0.13";

pub use isoloom_plan::routing::offline;

#[cfg(test)]
mod tests {
    #[test]
    fn a_path_relative_to_a_folder() {
        assert_eq!(super::relative_to("app", "build/shop/Dockerfile"), "../build/shop/Dockerfile");
        assert_eq!(super::relative_to("build/web", "build/web/Dockerfile.dev"), "Dockerfile.dev");
        assert_eq!(super::relative_to(".", "docker/Dockerfile"), "docker/Dockerfile");
        assert_eq!(super::relative_to("./src/", "Dockerfile"), "../Dockerfile");
    }
}
