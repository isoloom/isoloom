//! A machine's own process on its Compose service: its command, its working directory, its
//! `before` scripts in front of the command, and its healthcheck beside the port probe.

use isoloom_plan::process::{DEFAULT_INTERVAL, DEFAULT_RETRIES, DEFAULT_START_PERIOD, DEFAULT_TIMEOUT};
use isoloom_plan::{Health, MachinePlan};
use isoloom_spec::model::Machine;

use crate::docker::{PROBE_DIR, ROOT, file_name, ready_test};
use crate::file::{Healthcheck, Service};

/// Where a machine sees its `before` scripts.
pub const BEFORE_DIR: &str = "/isoloom/before";

/// `$$`: a literal `$`, not Compose's interpolation.
fn literal(s: &str) -> String {
    s.replace('$', "$$")
}

/// The command (the image's entrypoint runs it), the working directory, and the `before`
/// scripts: a shell entrypoint runs them in order, then hands over to the command (`exec`), so
/// the command is the machine's process as the image's would be.
pub(super) fn apply(mp: &MachinePlan, svc: &mut Service) {
    let p = &mp.process;
    if let Some(c) = &p.command {
        svc.command = c.argv().iter().map(|a| literal(a)).collect();
    }
    svc.working_dir.clone_from(&p.working_dir);
    if p.before.is_empty() {
        return;
    }
    let mut steps: Vec<String> = vec!["set -e".into()];
    for (i, script) in p.before.iter().enumerate() {
        let inside = format!("{BEFORE_DIR}/{}-{}", i + 1, file_name(script));
        steps.push(format!("sh {inside}"));
        svc.volumes.push(format!("{ROOT}/{script}:{inside}:ro"));
    }
    steps.push("exec \"$$@\"".into());
    svc.entrypoint = vec!["/bin/sh".into(), "-c".into(), steps.join("; "), "isoloom-before".into()];
}

/// The healthcheck of a machine: Isoloom's port probe and its own healthcheck, both run by
/// Isoloom's busybox (`sh`, and `wget` for a page), at its own pace when it has one. `None`
/// when there is nothing to check.
pub(super) fn healthcheck(m: &Machine, mp: &MachinePlan) -> Option<Healthcheck> {
    let health = mp.process.health.as_ref();
    let test = ready_test(m, health);
    if test.is_empty() {
        return None;
    }
    let secs = |s: u32| format!("{s}s");
    let pace = |f: fn(&Health) -> u32, default: u32| health.map_or(default, f);
    Some(Healthcheck {
        test: vec!["CMD".into(), format!("{PROBE_DIR}/busybox"), "sh".into(), "-c".into(), literal(&test)],
        interval: secs(pace(|h| h.interval, DEFAULT_INTERVAL)),
        timeout: secs(pace(|h| h.timeout, DEFAULT_TIMEOUT)),
        retries: pace(|h| h.retries, DEFAULT_RETRIES),
        start_period: Some(secs(pace(|h| h.start_period, DEFAULT_START_PERIOD))),
    })
}
