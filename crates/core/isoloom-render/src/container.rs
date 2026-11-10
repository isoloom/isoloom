//! What every target that runs the machines as containers shares, whichever backend writes the
//! files (Compose, Kubernetes) or builds the host they run on (a cloud VM, a local VM): the
//! images Isoloom brings and the readiness probe here; the one-shot jobs and how a Compose
//! project starts (`jobs`); the project files read into Compose's environment
//! (`project_files`); where the containers' host folders are bound from ([`host_folders`]); the size of the VM the containers run on (`host`); the containers' side
//! of a hybrid environment (`hybrid`); the access machine's stand-in and what is done to it around
//! the checks ([`STAND_INS_UP`], [`STAND_INS_DOWN`]). Pure functions over the spec and the plan.

mod host;
pub mod host_folders;
mod hybrid;
mod jobs;
mod project_files;

pub use host::HostSize;
pub use hybrid::{HybridContainers, docker_net};
pub use jobs::{as_arguments, init_names, leaf_jobs, start_commands};
pub use project_files::{copied_from_project, host_environment, project_variables, variable};

use isoloom_manifest::Failure;
use isoloom_plan::Health;
use isoloom_plan::checks::Position;
use isoloom_spec::model::Machine;

/// The check runner: a small image with `sh`, `curl` and busybox `nc`. Each helper image is pinned
/// by the digest of its multi-arch index: what runs never changes under a tag.
pub const CHECK_IMAGE: &str = "curlimages/curl:8.22.0@sha256:58adaa4e8dca9c988bae2aba4ab3434a0bb2da16bbe3f92dec39ec7785166777";

/// A static busybox Isoloom brings into every machine with services, so its probe needs
/// nothing from the machine's image (distroless and `scratch` images have no shell).
pub const PROBE_IMAGE: &str = "busybox:1.37.0-musl@sha256:f6a95d611e0a8780fabdb2aec43e31275b2720dfefa3e41b16835bfc42454478";
/// Where a machine sees that busybox (read-only).
pub const PROBE_DIR: &str = "/.isoloom-probe";

/// A small image with busybox `ip`, for the router, route sidecars and the stand-in.
pub const UTILITY_IMAGE: &str = "alpine:3.24@sha256:f59abe656b692d901e184be3445e084786602108599d2a51d229c0f5a48c71bf";

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

/// The name of the runner of a machine's `exec` checks, `isoloom-exec-<machine>`: under
/// Isoloom's own prefix, since a position's runner is named after its machine, which never takes
/// it.
#[must_use]
pub fn exec_runner_name(machine: &str) -> String {
    format!("isoloom-exec-{machine}")
}

/// The script of the runner of a machine's `exec` checks ([`exec_runner_name`]), in
/// `.isoloom/docker/checks/`: piped into the machine (`docker compose exec -T <machine> sh -s <
/// isoloom-exec-<machine>.sh`).
#[must_use]
pub fn exec_runner(machine: &str) -> String {
    format!("{}.sh", exec_runner_name(machine))
}

/// The service that stands in for the access machine while the checks run, so checks run from its
/// seat: the access machine is on demand (the `access` profile), so the checks can't count on it
/// running, and the stand-in, at its addresses, is never up beside it (only the `check` profile
/// starts it, and the checks' teardown takes it down).
pub const STAND_IN: &str = "isoloom-access";

/// The prefix of every check runner's service ([`check_runner`]).
pub const CHECK_RUNNER_PREFIX: &str = "isoloom-check";

/// The check runner of a position: `isoloom-check` where a user stands (`default`),
/// `isoloom-check-<position>` for the others. The one rule the Compose services, the resolved
/// snapshot and the run manifest name them by.
#[must_use]
pub fn check_runner(pos: &Position, default: &Position) -> String {
    if pos == default {
        CHECK_RUNNER_PREFIX.to_owned()
    } else {
        format!("{CHECK_RUNNER_PREFIX}-{}", pos.id())
    }
}

/// The VM the containers run in (`docker-vm`'s, `cloud-docker`'s, the hybrid target's), as run
/// manifests and Vagrantfiles name it: Isoloom's own, under the prefix a lab's machines can't
/// take, so it never collides with one (a machine named `docker` included).
pub const DOCKER_HOST: &str = "isoloom-docker-host";

/// What is done to the access machine's stand-ins around the checks, the same on this computer and
/// in a VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StandInStep {
    /// The step's id.
    pub id: &'static str,
    /// What `docker compose --profile check` does to them: the subcommand and its flags.
    pub verb: &'static [&'static str],
    /// What the step says it is doing.
    pub note: Option<&'static str>,
    /// How long the step may run, in seconds.
    pub timeout_s: u32,
    /// What its failure does: a stand-in that failed to start stops the checks; one left
    /// behind is only logged.
    pub failure: Failure,
}

impl StandInStep {
    /// Compose's arguments for `stand_ins`: `--profile check <verb> <stand-ins>`, the same on this
    /// computer and in a VM.
    #[must_use]
    pub fn args<'a>(&self, stand_ins: &'a [String]) -> Vec<&'a str> {
        let mut args = vec!["--profile", "check"];
        args.extend(self.verb);
        args.extend(stand_ins.iter().map(String::as_str));
        args
    }
}

/// Before the runners: the stand-ins started.
pub const STAND_INS_UP: StandInStep = StandInStep {
    id: "stand-ins",
    verb: &["up", "--detach", "--wait", "--no-deps"],
    note: Some("Starting the access machine's stand-in…"),
    timeout_s: 600,
    failure: Failure::Fail,
};

/// After the runners, whatever happened: the stand-ins taken down, so the access machine they
/// stand in for (brought up on demand) can take its addresses. A failure there doesn't fail the
/// checks, which already ran.
pub const STAND_INS_DOWN: StandInStep = StandInStep {
    id: "stand-ins-down",
    verb: &["rm", "--stop", "--force"],
    note: None,
    timeout_s: 120,
    failure: Failure::Tolerate,
};

/// The network sidecar of a machine (or of the [`STAND_IN`]), which sets its routes:
/// `isoloom-routes-<machine>`, under the prefix a lab's machines can't take, so no machine's name
/// is another's sidecar.
#[must_use]
pub fn routes_service(machine: &str) -> String {
    format!("isoloom-routes-{machine}")
}

#[cfg(test)]
mod tests;
