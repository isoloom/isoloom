//! A machine's own process on its Compose service: its command, its working directory, and its
//! healthcheck beside the port probe.

use isoloom_plan::process::{DEFAULT_INTERVAL, DEFAULT_RETRIES, DEFAULT_START_PERIOD, DEFAULT_TIMEOUT};
use isoloom_plan::{Health, MachinePlan};
use isoloom_render::container::{PROBE_DIR, ready_test};
use isoloom_spec::model::Machine;

use crate::file::{Healthcheck, Seconds, Service};
use crate::interpolation::literal;

/// The command (the image's entrypoint runs it) and the working directory, so the command is
/// the machine's process as the image's would be.
pub(super) fn apply(mp: &MachinePlan, svc: &mut Service) {
    let p = &mp.process;
    if let Some(c) = &p.command {
        svc.command = c.argv().iter().map(|a| literal(a)).collect();
    }
    svc.working_dir.clone_from(&p.working_dir);
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
    let pace = |f: fn(&Health) -> u32, default: u32| health.map_or(default, f);
    Some(Healthcheck {
        test: vec!["CMD".into(), format!("{PROBE_DIR}/busybox"), "sh".into(), "-c".into(), literal(&test)],
        interval: Seconds(pace(|h| h.interval, DEFAULT_INTERVAL)),
        timeout: Seconds(pace(|h| h.timeout, DEFAULT_TIMEOUT)),
        retries: pace(|h| h.retries, DEFAULT_RETRIES),
        start_period: Some(Seconds(pace(|h| h.start_period, DEFAULT_START_PERIOD))),
    })
}
