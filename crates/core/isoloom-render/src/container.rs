//! What every target that runs the machines as containers shares, whichever backend writes the
//! files (Compose, Kubernetes) or builds the host they run on (a cloud VM, a local VM): the
//! images Isoloom brings and the readiness probe here; the one-shot jobs and how a Compose
//! project starts (`jobs`); the project files read into Compose's environment
//! (`project_files`); the size of the VM the containers run on (`host`); the containers' side
//! of a hybrid environment (`hybrid`). Pure functions over the spec and the plan.

mod host;
mod hybrid;
mod jobs;
mod project_files;

pub use host::HostSize;
pub use hybrid::{HybridContainers, docker_net};
pub use jobs::{as_arguments, init_names, leaf_jobs, start_commands};
pub use project_files::{copied_from_project, host_environment, project_variables, variable};

use isoloom_plan::Health;
use isoloom_spec::model::Machine;

/// The check runner: a small image with `sh`, `curl` and busybox `nc`.
pub const CHECK_IMAGE: &str = "curlimages/curl:8.11.1";

/// A static busybox Isoloom brings into every machine with services, so its probe needs
/// nothing from the machine's image (distroless and `scratch` images have no shell).
pub const PROBE_IMAGE: &str = "busybox:1.37.0-musl";
/// Where a machine sees that busybox (read-only).
pub const PROBE_DIR: &str = "/.isoloom-probe";

/// A small image with busybox `ip`, for the router, route sidecars and the stand-in.
pub const UTILITY_IMAGE: &str = "alpine:3.20";

/// A TCP probe for each service port but the dormant ones, run by Isoloom's own busybox (see
/// [`PROBE_IMAGE`]).
#[must_use]
pub fn probe(m: &Machine) -> String {
    m.ready_ports()
        .iter()
        .map(|port| format!("{PROBE_DIR}/busybox nc -z -w 2 127.0.0.1 {port}"))
        .collect::<Vec<_>>()
        .join(" && ")
}

/// Everything a machine's readiness checks, as one line for Isoloom's busybox `sh`: its ports
/// ([`probe`]), then its own healthcheck (`health`; a page through busybox `wget`), grouped so
/// its own `||` stays its own. Empty when there is neither.
#[must_use]
pub fn ready_test(m: &Machine, health: Option<&Health>) -> String {
    let own = health.map(|h| format!("( {} )", h.shell_test(&format!("{PROBE_DIR}/busybox wget"))));
    [probe(m)].into_iter().filter(|t| !t.is_empty()).chain(own).collect::<Vec<_>>().join(" && ")
}

/// The runner of a machine's `exec` checks, in `.isoloom/docker/checks/`: piped into the
/// machine (`docker compose exec -T <machine> sh -s < exec-<machine>.sh`).
#[must_use]
pub fn exec_runner(machine: &str) -> String {
    format!("exec-{machine}.sh")
}

#[cfg(test)]
mod tests;
