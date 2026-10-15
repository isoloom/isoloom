//! The `docker` target: one Compose file at `.isoloom/docker/compose.yml`.
//!
//! - Each network becomes a Compose network with its subnet (Docker at `.1`, or at the last
//!   address when a machine is the network's gateway); `internet: false` makes it `internal`
//!   when nothing routes between networks.
//! - A machine that is a network's gateway gets forwarding and `NET_ADMIN`; the machines
//!   behind it route everything through it (in a sidecar, once it answers).
//! - Each machine becomes a service with its fixed address on every network; services find
//!   each other by name (Compose DNS).
//! - Machines with services get a healthcheck (TCP probe; and the machine's own
//!   `healthcheck` beside it), so `depends_on` waits for them
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

use isoloom_plan::checks::Position;
use serde_yaml_ng::{Mapping, Value};

use crate::file::ComposeFile;
use crate::model::{Arch, Machine};

pub(crate) const DIR: &str = "docker";
/// From `.isoloom/docker/` back to the project folder.
pub(crate) const ROOT: &str = "../..";
/// On everything an environment creates on Docker: `isoloom gc` collects only what has it.
pub const MANAGED_LABEL: &str = "isoloom.managed";
/// The environment a Docker resource belongs to.
pub const ENVIRONMENT_LABEL: &str = "isoloom.environment";

/// The one-shot service (Compose) that copies the probe for machines of this architecture.
pub fn probe_service(arch: Arch) -> String {
    format!("isoloom-probe-{}", arch.id())
}

/// What a machine that waits for `m` waits for: healthy (its probe passes) when it has a port
/// to probe or a healthcheck of its own, else started.
pub fn ready_condition(m: &Machine) -> &'static str {
    if m.has_readiness() { "service_healthy" } else { "service_started" }
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

/// A service as the start plan reads it: its name, what it waits for, whether it runs once
/// (`restart: "no"`). Services in a profile aren't started, so they aren't read.
struct Started {
    name: String,
    deps: Vec<String>,
    one_shot: bool,
}

/// The [`StartPlan`] of a Compose file: one-shots (`restart: "no"`) outside profiles that no
/// other started service depends on are jobs; when there are none, `wait` is empty (start all).
pub fn start_plan(compose_yaml: &str) -> Result<StartPlan, String> {
    let doc: Value = serde_yaml_ng::from_str(compose_yaml).map_err(|e| e.to_string())?;
    let Some(Value::Mapping(services)) = doc.get("services") else {
        return Ok(StartPlan::default());
    };
    let deps_of = |v: &Mapping| -> Vec<String> {
        match v.get("depends_on") {
            Some(Value::Mapping(d)) => d.keys().filter_map(|k| k.as_str().map(String::from)).collect(),
            Some(Value::Sequence(d)) => d.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            _ => Vec::new(),
        }
    };
    let started: Vec<Started> = services
        .iter()
        .filter_map(|(k, v)| Some((k.as_str()?, v.as_mapping()?)))
        .filter(|(_, v)| v.get("profiles").is_none())
        .map(|(name, v)| Started {
            name: name.to_string(),
            deps: deps_of(v),
            one_shot: v.get("restart").and_then(Value::as_str) == Some("no"),
        })
        .collect();
    Ok(plan_of(&started))
}

impl StartPlan {
    /// The plan of a typed Compose file: what [`start_plan`] reads from its text.
    pub fn of(file: &ComposeFile) -> StartPlan {
        let started: Vec<Started> = file
            .services
            .iter()
            .filter(|(_, s)| s.profiles.is_empty())
            .map(|(name, s)| Started {
                name: name.clone(),
                deps: s.depends_on.keys().cloned().collect(),
                one_shot: s.restart.as_deref() == Some("no"),
            })
            .collect();
        plan_of(&started)
    }
}

fn plan_of(started: &[Started]) -> StartPlan {
    // What the long-running services need, through chains of one-shots: `--wait` accepts those
    // exiting. The other one-shots are the jobs.
    let mut needed: Vec<&str> = Vec::new();
    let mut todo: Vec<&str> = started.iter().filter(|s| !s.one_shot).flat_map(|s| s.deps.iter().map(String::as_str)).collect();
    while let Some(n) = todo.pop() {
        if needed.contains(&n) {
            continue;
        }
        if let Some(s) = started.iter().find(|s| s.name == n) {
            todo.extend(s.deps.iter().map(String::as_str));
        }
        needed.push(n);
    }
    let jobs: Vec<String> = started
        .iter()
        .filter(|s| s.one_shot && !needed.contains(&s.name.as_str()))
        .map(|s| s.name.clone())
        .collect();
    if jobs.is_empty() {
        return StartPlan::default();
    }
    let wait = started.iter().map(|s| s.name.clone()).filter(|n| !jobs.contains(n)).collect();
    StartPlan { wait, jobs }
}

/// The check runner service of a position: `isoloom-check` where a user stands (`default`),
/// `isoloom-check-<position>` for the others.
pub fn check_runner_service(pos: &Position, default: &Position) -> String {
    if pos == default {
        "isoloom-check".to_string()
    } else {
        format!("isoloom-check-{}", pos.id())
    }
}

/// The service of a tool (`tools:`).
pub fn tool_service(tool: &str) -> String {
    format!("isoloom-tool-{tool}")
}

/// The last component of a `/`-separated path (a script's name in the container).
pub fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The `shell` tool's image: a toolbox with tcpdump, nmap, curl, dig, netcat and more.
pub const TOOLBOX_IMAGE: &str = "nicolaka/netshoot";

/// Stands in for an access machine the runner supplies, so checks run from its side.
pub(crate) const STAND_IN: &str = "isoloom-access";
/// The sidecar image of a machine on a network with `tc`: one with `tc` (the utility image has
/// none, and an offline machine couldn't install it).
pub(crate) const TC_IMAGE: &str = "nicolaka/netshoot:v0.13";

pub use isoloom_plan::routing::offline;
pub(crate) use isoloom_render::container::init_names;
pub use isoloom_render::container::{CHECK_IMAGE, PROBE_DIR, PROBE_IMAGE, UTILITY_IMAGE, exec_runner, leaf_jobs, probe, ready_test, start_commands};

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
