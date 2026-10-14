//! A machine's own process (`command`, `working_dir`, `before`, `healthcheck`): what each
//! rendering does with it. A container runs it as its image's command would run; a VM runs it as
//! a service and tries the healthcheck before it is marked ready; an existing host is left as it
//! is, so `external` refuses it.

use crate::spec::status::PROBE;
use crate::spec::status::Status::{self, Done, Planned};

/// The Compose file's cells.
pub const COMPOSE: &[(&str, Status)] = &[
    ("machines.*.command", done("the service's `command` (the image's entrypoint runs it)")),
    ("machines.*.working_dir", done("the service's `working_dir`")),
    (
        "machines.*.before",
        done("a shell `entrypoint` runs the scripts (mounted read-only), then hands over to the command"),
    ),
    (
        "machines.*.healthcheck.test",
        done("the `healthcheck` test, after the port probe, run by Isoloom's busybox"),
    ),
    (
        "machines.*.healthcheck.http.port",
        done("busybox `wget` of the page, in the `healthcheck` test"),
    ),
    (
        "machines.*.healthcheck.http.path",
        done("busybox `wget` of the page, in the `healthcheck` test"),
    ),
    ("machines.*.healthcheck.interval", done("the healthcheck's `interval`")),
    ("machines.*.healthcheck.timeout", done("the healthcheck's `timeout`")),
    ("machines.*.healthcheck.retries", done("the healthcheck's `retries`")),
    ("machines.*.healthcheck.start_period", done("the healthcheck's `start_period`")),
];

/// The Kubernetes manifests' cells.
pub const KUBERNETES: &[(&str, Status)] = &[
    ("machines.*.command", done("the container's `args` (the image's entrypoint runs them)")),
    ("machines.*.working_dir", done("the container's `workingDir`")),
    (
        "machines.*.before",
        done("the container's `command`: a shell runs the scripts (from the scripts ConfigMap), then hands over to the args"),
    ),
    ("machines.*.healthcheck.test", done("an `exec` readiness probe, after the port probe")),
    (
        "machines.*.healthcheck.http.port",
        done("an `httpGet` readiness probe, the port probe as the startup probe"),
    ),
    (
        "machines.*.healthcheck.http.path",
        done("an `httpGet` readiness probe, the port probe as the startup probe"),
    ),
    ("machines.*.healthcheck.interval", done("the readiness probe's `periodSeconds`")),
    ("machines.*.healthcheck.timeout", done("the readiness probe's `timeoutSeconds`")),
    ("machines.*.healthcheck.retries", done("the readiness probe's `failureThreshold`")),
    ("machines.*.healthcheck.start_period", done("the readiness probe's `initialDelaySeconds`")),
];

/// Every VM output's cells (Vagrant, Proxmox, the clouds): the plan's `Before`, `Command` and
/// `Ready` steps.
pub const VM: &[(&str, Status)] = &[
    (
        "machines.*.command",
        done("a service restarted when it fails (systemd, OpenRC on Alpine, a startup task on Windows)"),
    ),
    ("machines.*.working_dir", done("where the command runs (default: the project folder)")),
    ("machines.*.before", done("a one-shot service before the command, run now and at every boot")),
    (
        "machines.*.healthcheck.test",
        done("tried in the `Ready` step: the set-up fails when it doesn't pass"),
    ),
    ("machines.*.healthcheck.http.port", done("the page fetched in the `Ready` step (curl or wget)")),
    ("machines.*.healthcheck.http.path", done("the page fetched in the `Ready` step (curl or wget)")),
    ("machines.*.healthcheck.interval", done("the pause between the `Ready` step's tries")),
    ("machines.*.healthcheck.timeout", done("how long one of the `Ready` step's tries may take")),
    ("machines.*.healthcheck.retries", done("the `Ready` step's tries")),
    ("machines.*.healthcheck.start_period", done("the wait before the `Ready` step's first try")),
];

/// The `external` output's cells: what a service over SSH would do; its backend refuses them.
pub const EXTERNAL: &[(&str, Status)] = &[
    ("machines.*.command", HOST),
    ("machines.*.working_dir", HOST),
    ("machines.*.before", HOST),
    ("machines.*.healthcheck.test", HOST),
    ("machines.*.healthcheck.http.port", HOST),
    ("machines.*.healthcheck.http.path", HOST),
    ("machines.*.healthcheck.interval", HOST),
    ("machines.*.healthcheck.timeout", HOST),
    ("machines.*.healthcheck.retries", HOST),
    ("machines.*.healthcheck.start_period", HOST),
];

/// An existing host's process is its own: a service over SSH would change the host.
const HOST: Status = Planned {
    note: "a service of Isoloom's on the existing host, over SSH",
};

/// Produced as `how` says, shown by the field's probe.
const fn done(how: &'static str) -> Status {
    Done { how, proof: PROBE }
}

#[cfg(test)]
mod tests {
    use super::{COMPOSE, done};
    use crate::spec::status::{PROBE, Status};

    /// The cells are built at compile time; building one again at run time proves the helper
    /// (and lets line coverage see it run).
    #[test]
    fn done_is_proven_by_the_field_s_probe() {
        let how = "the service's `command` (the image's entrypoint runs it)";
        assert_eq!(done(how), Status::Done { how, proof: PROBE });
        assert_eq!(COMPOSE[0].1, done(how));
    }
}
