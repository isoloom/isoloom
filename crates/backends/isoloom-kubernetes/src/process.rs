//! A machine's own process in its container: its command as the container's `args` (the
//! image's entrypoint runs it), its working directory, and its readiness: Isoloom's port probe and the machine's own healthcheck.
//!
//! A command healthcheck joins the port probe in one `exec` probe. A page is an `httpGet`
//! probe; the port probe then comes first, as the startup probe (the readiness probe only starts
//! once the startup probe has passed), so the machine is still ready only when both pass.

use isoloom_plan::process::{DEFAULT_INTERVAL, DEFAULT_RETRIES};
use isoloom_plan::{Health, HealthTest};
use isoloom_render::container::PROBE_DIR;

use crate::machine::Machine;
use crate::objects::{Container, Exec, HttpGet, Probe};

/// Where the project's scripts are, in a container running them.
pub(crate) const SCRIPTS_DIR: &str = "/isoloom/scripts";

/// The command and the working directory on the machine's container.
pub(crate) fn apply(m: &Machine, c: &mut Container) {
    let p = &m.plan.process;
    if let Some(command) = &p.command {
        c.args = command.argv();
    }
    c.working_dir.clone_from(&p.working_dir);
}

/// Everything the machine's readiness checks, as one shell line for Isoloom's busybox: its
/// ports, then its own healthcheck. Empty when it has neither.
pub(crate) fn ready_test(m: &Machine) -> String {
    isoloom_render::container::ready_test(m.spec, m.plan.process.health.as_ref())
}

/// A command run by Isoloom's busybox shell.
fn busybox(line: String) -> Exec {
    Exec {
        command: vec![format!("{PROBE_DIR}/busybox"), "sh".into(), "-c".into(), line],
    }
}

/// The startup and readiness probes, when the machine has something to probe.
pub(crate) fn probes(m: &Machine, c: &mut Container) {
    let ports = isoloom_render::container::probe(m.spec);
    let Some(h) = m.plan.process.health.as_ref() else {
        if !ports.is_empty() {
            c.readiness_probe = Some(Probe {
                exec: Some(busybox(ports)),
                period_seconds: DEFAULT_INTERVAL,
                failure_threshold: DEFAULT_RETRIES,
                ..Probe::default()
            });
        }
        return;
    };
    let mut probe = paced(h);
    match &h.test {
        HealthTest::Command(_) => probe.exec = Some(busybox(ready_test(m))),
        HealthTest::Http { port, path } => {
            probe.http_get = Some(HttpGet {
                path: path.clone(),
                port: *port,
            });
            if !ports.is_empty() {
                c.startup_probe = Some(Probe {
                    exec: Some(busybox(ports)),
                    period_seconds: DEFAULT_INTERVAL,
                    failure_threshold: DEFAULT_RETRIES,
                    ..Probe::default()
                });
            }
        }
    }
    c.readiness_probe = Some(probe);
}

/// A probe at the healthcheck's pace: one passing try makes the machine ready, as Docker's.
fn paced(h: &Health) -> Probe {
    Probe {
        initial_delay_seconds: Some(h.start_period),
        period_seconds: h.interval,
        timeout_seconds: Some(h.timeout),
        success_threshold: Some(1),
        failure_threshold: h.retries,
        ..Probe::default()
    }
}
