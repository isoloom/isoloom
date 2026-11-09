//! The check runners of a Compose run: one `compose run` per position, plus a machine's own
//! `exec` checks piped into it; each with the checks it reports and a time limit that covers
//! their waits.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use isoloom_manifest::{Action, CheckRunner, Failure, Name, PathTemplate, RelPath, SnakeName, Step, StepId, Template};
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

/// Every position's checks, in the order `isoloom test` runs them.
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

/// Every position's runners, in the order `isoloom test` runs them.
pub(super) fn runners(env: &Environment) -> Result<Runners> {
    let mut out = Runners {
        runners: Vec::new(),
        derived: Vec::new(),
    };
    for p in planned(env) {
        let runner = compose_runner(&p.pos, &p.service, &p.checks)?;
        out.derived.push(runner.step.id.clone());
        out.runners.push(runner);
        if let Some((m, execs)) = &p.execs {
            out.runners.push(exec_runner_of(&p.pos, m, execs)?);
        }
    }
    Ok(out)
}

fn compose_runner(pos: &Position, service: &str, group: &[&Resolved]) -> Result<CheckRunner> {
    let id = format!("check-{}", pos.id());
    let args = [
        "--profile",
        "check",
        "run",
        "--rm",
        "--no-deps",
        "--env",
        isoloom_render::run_manifest::DERIVED,
        service,
    ];
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
    step.stdin = Some(PathTemplate::relative(&RelPath::parse(&format!("checks/{}", exec_runner(machine)))?));
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

/// What is done to the access machine's stand-ins around the checks.
pub(super) struct StandInStep {
    /// The step's id.
    pub(super) id: &'static str,
    /// What `docker compose --profile check` does to them: the subcommand and its flags.
    pub(super) verb: &'static [&'static str],
    /// What the step says it is doing.
    pub(super) note: Option<&'static str>,
    /// How long the step may run, in seconds.
    pub(super) timeout_s: u32,
    /// What its failure does: a stand-in that failed to start stops the checks; one left
    /// behind is only logged.
    pub(super) failure: Failure,
}

impl StandInStep {
    /// Compose's arguments for `stand_ins`: `--profile check <verb> <stand-ins>`, the same on this
    /// computer and in a VM.
    pub(super) fn args<'a>(&self, stand_ins: &'a [String]) -> Vec<&'a str> {
        let mut args = vec!["--profile", "check"];
        args.extend(self.verb);
        args.extend(stand_ins.iter().map(String::as_str));
        args
    }
}

/// Before the runners: the stand-ins started.
pub(super) const STAND_INS_UP: StandInStep = StandInStep {
    id: "stand-ins",
    verb: &["up", "--detach", "--wait", "--no-deps"],
    note: Some("Starting the access machine's stand-in…"),
    timeout_s: 600,
    failure: Failure::Fail,
};

/// After the runners, whatever happened: the stand-ins taken down, so the access machine they
/// stand in for (brought up on demand) can take its addresses. A failure there doesn't fail the
/// checks, which already ran.
pub(super) const STAND_INS_DOWN: StandInStep = StandInStep {
    id: "stand-ins-down",
    verb: &["rm", "--stop", "--force"],
    note: None,
    timeout_s: 120,
    failure: Failure::Tolerate,
};

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
