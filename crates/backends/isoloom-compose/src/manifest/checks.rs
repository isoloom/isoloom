//! The check runners of a Compose run: one `compose run` per position, plus a machine's own
//! `exec` checks piped into it; each with the checks it reports and a time limit that covers
//! their waits.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use isoloom_manifest::{Action, CheckRunner, Failure, Name, RelPath, SnakeName, Step, StepId, Template};
use isoloom_plan::Environment;
use isoloom_plan::checks::{self, Position, Probe, Resolved};
use isoloom_render::run_manifest::{check, timeout_s};

use isoloom_render::container::exec_runner;

use super::compose;
use crate::error::Result;
use crate::file::ComposeFile;
use crate::names::{STAND_IN, check_runner_service, routes_service};

/// The runners, and the steps that read `ISOLOOM_DERIVED`.
pub(super) struct Runners {
    /// The runners, position by position.
    pub(super) runners: Vec<CheckRunner>,
    /// The `compose run` steps, which pass `ISOLOOM_DERIVED` to their runner.
    pub(super) derived: Vec<StepId>,
}

/// Every position's runners, in the order `isoloom test` runs them.
pub(super) fn runners(env: &Environment) -> Result<Runners> {
    let spec = env.spec();
    let default = checks::default_position(spec);
    let mut out = Runners {
        runners: Vec::new(),
        derived: Vec::new(),
    };
    for (pos, group) in checks::by_position(spec, &env.checks) {
        let (execs, others): (Vec<&Resolved>, Vec<&Resolved>) = group.into_iter().partition(|c| matches!(c.probe, Probe::Exec { .. }));
        let runner = compose_runner(&pos, &check_runner_service(&pos, &default), &others)?;
        out.derived.push(runner.step.id.clone());
        out.runners.push(runner);
        if let (Position::Machine(m), false) = (&pos, execs.is_empty()) {
            out.runners.push(exec_runner_of(&pos, m, &execs)?);
        }
    }
    Ok(out)
}

fn compose_runner(pos: &Position, service: &str, group: &[&Resolved]) -> Result<CheckRunner> {
    let id = format!("check-{}", pos.id());
    let args = ["--profile", "check", "run", "--rm", "--no-deps", "--env", "ISOLOOM_DERIVED", service];
    let mut step = Step::argv(StepId::parse(&id)?, compose(&args)?);
    step.timeout_s = NonZeroU32::new(timeout_s(group));
    let mut rm = Step::action(StepId::parse(&format!("rm-{id}"))?, Action::ComposeRemoveContainers);
    rm.failure = Failure::Tolerate;
    if let isoloom_manifest::Run::Action { with, .. } = &mut rm.run {
        *with = BTreeMap::from([(arg("service")?, Template::text(service)), (arg("one_off")?, Template::text("true"))]);
    }
    Ok(CheckRunner {
        id: Name::parse(service)?,
        from: pos.label(),
        checks: group.iter().map(|c| check(c)).collect(),
        step,
        on_timeout: vec![rm],
    })
}

/// A machine's `exec` checks: their script piped into the machine itself.
fn exec_runner_of(pos: &Position, machine: &str, execs: &[&Resolved]) -> Result<CheckRunner> {
    let id = format!("check-exec-{}", pos.id());
    let mut step = Step::argv(StepId::parse(&id)?, compose(&["exec", "-T", machine, "sh", "-s"])?);
    step.stdin = Some(RelPath::parse(&format!("checks/{}", exec_runner(machine)))?);
    step.timeout_s = NonZeroU32::new(timeout_s(execs));
    Ok(CheckRunner {
        id: Name::parse(&format!("exec-{}", pos.id()))?,
        from: format!("inside {machine}"),
        checks: execs.iter().map(|c| check(c)).collect(),
        step,
        on_timeout: Vec::new(),
    })
}

fn arg(name: &str) -> Result<SnakeName> {
    Ok(SnakeName::parse(name)?)
}

/// Before the runners: the stand-in for a supplied access machine, and its routes, when the
/// file has them (only the check profile starts them).
pub(super) fn setup(file: &ComposeFile) -> Result<Vec<Step>> {
    let routes = routes_service(STAND_IN);
    let stand_ins: Vec<&str> = [STAND_IN, routes.as_str()].into_iter().filter(|s| file.services.contains_key(*s)).collect();
    if stand_ins.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec!["--profile", "check", "up", "--detach", "--wait", "--no-deps"];
    args.extend(stand_ins);
    let mut step = Step::argv(StepId::parse("stand-ins")?, compose(&args)?);
    step.note = Some("Starting the access machine's stand-in…".into());
    step.timeout_s = NonZeroU32::new(600);
    Ok(vec![step])
}
