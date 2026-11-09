//! The check runners of a Compose run: one `compose run` per position, plus a machine's own
//! `exec` checks piped into it; each with the checks it reports and a time limit that covers
//! their waits.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use isoloom_manifest::{Action, CheckRunner, Failure, Name, PathTemplate, RelPath, SnakeName, Step, StepId, Template};
use isoloom_plan::Environment;
use isoloom_plan::checks::{self, Position, Probe, Resolved};
use isoloom_render::run_manifest::{DERIVED, check, timeout_s};

use isoloom_render::container::{STAND_INS_DOWN, STAND_INS_UP, StandInStep, exec_runner};

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

/// A position's checks as Compose runs them: the `check` profile's service running them, and a
/// machine's own `exec` checks, piped into the machine.
pub(super) struct Planned<'a> {
    /// Where they run from.
    pub(super) pos: Position,
    /// The runner's service.
    pub(super) service: String,
    /// The checks it runs.
    pub(super) checks: Vec<&'a Resolved>,
    /// The machine whose `exec` checks are piped into it, and those checks.
    pub(super) execs: Option<(String, Vec<&'a Resolved>)>,
}

impl Planned<'_> {
    /// The id of the step running the position's runner (`check-<position>`).
    pub(super) fn step_id(&self) -> String {
        format!("check-{}", self.pos.id())
    }

    /// The id of the step clearing what the runner leaves when its time limit cuts it off.
    pub(super) fn leftover_id(&self) -> String {
        format!("rm-{}", self.step_id())
    }

    /// The id of the runner of the machine's own `exec` checks (`exec-<position>`).
    pub(super) fn exec_id(&self) -> String {
        format!("exec-{}", self.pos.id())
    }

    /// The id of the step piping the `exec` checks into the machine.
    pub(super) fn exec_step_id(&self) -> String {
        format!("check-exec-{}", self.pos.id())
    }
}

/// Where a machine's own `exec` checks run from, as a runner's `from` says it.
pub(super) fn inside(machine: &str) -> String {
    format!("inside {machine}")
}

/// Every position's checks, in the order the runner's `check` runs them.
pub(super) fn planned(env: &Environment) -> Vec<Planned<'_>> {
    let spec = env.spec();
    let default = checks::default_position(spec);
    checks::by_position(spec, &env.checks)
        .into_iter()
        .map(|(pos, group)| {
            let (execs, others): (Vec<&Resolved>, Vec<&Resolved>) = group.into_iter().partition(|c| matches!(c.probe, Probe::Exec { .. }));
            let execs = match &pos {
                Position::Machine(m) if !execs.is_empty() => Some((m.clone(), execs)),
                _ => None,
            };
            Planned {
                service: check_runner_service(&pos, &default),
                pos,
                checks: others,
                execs,
            }
        })
        .collect()
}

/// Every position's runners, in the order the runner's `check` runs them.
pub(super) fn runners(env: &Environment) -> Result<Runners> {
    let mut out = Runners {
        runners: Vec::new(),
        derived: Vec::new(),
    };
    for p in planned(env) {
        let runner = compose_runner(&p)?;
        out.derived.push(runner.step.id.clone());
        out.runners.push(runner);
        if let Some((m, execs)) = &p.execs {
            out.runners.push(exec_runner_of(&p, m, execs)?);
        }
    }
    Ok(out)
}

fn compose_runner(p: &Planned<'_>) -> Result<CheckRunner> {
    let args = ["--profile", "check", "run", "--rm", "--no-deps", "--env", DERIVED, &p.service];
    let mut step = Step::argv(StepId::parse(&p.step_id())?, compose(&args)?);
    step.timeout_s = NonZeroU32::new(timeout_s(&p.checks));
    let mut rm = Step::action(StepId::parse(&p.leftover_id())?, Action::ComposeRemoveContainers);
    rm.failure = Failure::Tolerate;
    if let isoloom_manifest::Run::Action { with, .. } = &mut rm.run {
        *with = BTreeMap::from([(arg("service")?, Template::text(&p.service)), (arg("one_off")?, Template::text("true"))]);
    }
    Ok(CheckRunner {
        id: Name::parse(&p.service)?,
        from: p.pos.label(),
        checks: p.checks.iter().map(|c| check(c)).collect(),
        step,
        on_timeout: vec![rm],
    })
}

/// A machine's `exec` checks: their script piped into the machine itself.
fn exec_runner_of(p: &Planned<'_>, machine: &str, execs: &[&Resolved]) -> Result<CheckRunner> {
    let mut step = Step::argv(StepId::parse(&p.exec_step_id())?, compose(&["exec", "-T", machine, "sh", "-s"])?);
    step.stdin = Some(PathTemplate::relative(&RelPath::parse(&format!("checks/{}", exec_runner(machine)))?));
    step.timeout_s = NonZeroU32::new(timeout_s(execs));
    Ok(CheckRunner {
        id: Name::parse(&p.exec_id())?,
        from: inside(machine),
        checks: execs.iter().map(|c| check(c)).collect(),
        step,
        on_timeout: Vec::new(),
    })
}

fn arg(name: &str) -> Result<SnakeName> {
    Ok(SnakeName::parse(name)?)
}

/// The stand-in for a supplied access machine, and its routes, when the file has them (only the
/// check profile starts them).
pub(super) fn stand_ins(file: &ComposeFile) -> Vec<String> {
    let routes = routes_service(STAND_IN);
    [STAND_IN, routes.as_str()]
        .into_iter()
        .filter(|s| file.services.contains_key(*s))
        .map(str::to_owned)
        .collect()
}

/// Before the runners: the stand-ins started.
pub(super) fn setup(file: &ComposeFile) -> Result<Vec<Step>> {
    stand_in_step(file, &STAND_INS_UP)
}

/// After the runners: the stand-ins taken down.
pub(super) fn teardown(file: &ComposeFile) -> Result<Vec<Step>> {
    stand_in_step(file, &STAND_INS_DOWN)
}

/// `docker compose --profile check <verb> <stand-ins>`, when the file has stand-ins.
fn stand_in_step(file: &ComposeFile, what: &StandInStep) -> Result<Vec<Step>> {
    let stand_ins = stand_ins(file);
    if stand_ins.is_empty() {
        return Ok(Vec::new());
    }
    let mut step = Step::argv(StepId::parse(what.id)?, compose(&what.args(&stand_ins))?);
    step.note = what.note.map(str::to_owned);
    step.timeout_s = NonZeroU32::new(what.timeout_s);
    step.failure = what.failure;
    Ok(vec![step])
}
