//! Compose inside a VM (`docker-vm`): the `docker` target's Compose file run from the VM's copy of
//! the project, by shell commands the VM's backend runs through its own way in. The same services,
//! roles, slots, start plan and check runners as on this computer's Docker ([`super::docker`]),
//! with what differs in a VM: the file's own project name (one lab per VM), every published port
//! fixed at its `publish` number (the VM forwards it, or answers at its own address), the project
//! files Compose reads from its environment read from the VM's copy, and root's Docker (through
//! `sudo` keeping the values the way in forwarded, unless the way in is root: [`AsRoot`]).

use isoloom_manifest::contract::{OUTPUT_DIR, PUBLISH_FIXED};
use isoloom_manifest::{EnvName, Failure, Machine, Name, Part, Placeholder, PublishMode, StepId, Template};
use isoloom_plan::Environment;
use isoloom_render::container::{exec_runner, host_environment, host_folders};
use isoloom_render::run_manifest::guest::{GuestCommand, GuestCompose, GuestRunner};
use isoloom_render::run_manifest::{DERIVED, check, derived_switch, networks, timeout_s};
use isoloom_render::steps;
use isoloom_render::steps::AsRoot;

use super::Project;

use super::checks::{Planned, inside, planned, stand_ins};
use super::lifecycle::{self, ready};
use super::machines::{Member, classify, machines};
use super::slots::{Starts, slots};
use super::stand_ins::{STAND_INS_DOWN, STAND_INS_UP, StandInStep};
use crate::error::Result;
use crate::file::ComposeFile;
use crate::start::StartPlan;

/// The commands of a project in the VM: `docker compose` as root under its name, with its file,
/// its ports published on every address of the VM at their own numbers, and its containers' host
/// folders where the VM shares them.
struct InVm {
    project: String,
    /// How the way in gets root.
    as_root: AsRoot,
    /// `<variable>=<where the VM shares it> ` for each container's host folder.
    folders: String,
}

impl InVm {
    /// `cd <project copy> && <before>sudo -E … docker compose … <rest>`, `-E` keeping what the way
    /// in forwarded and `before` exported (no `sudo` for root).
    fn compose(&self, before: &str, rest: &[Part]) -> Template {
        let head = format!(
            "cd {} && {before}{}env ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 {PUBLISH_FIXED}=1 {}docker compose --project-name {} --file {OUTPUT_DIR}/docker/compose.yml ",
            steps::PROJECT_DIR,
            self.as_root.keeping_env(),
            self.folders,
            self.project
        );
        Template::from_parts(std::iter::once(Part::Text(head)).chain(rest.iter().cloned()))
    }

    /// The containers, as `ps` lists them: nothing before Docker is in the VM.
    fn status(&self) -> Template {
        Template::text(&format!(
            "if command -v docker >/dev/null; then {}docker compose --project-name {} ps --all --format json; fi",
            self.as_root.prefix(),
            self.project
        ))
    }
}

fn text(words: &str) -> Vec<Part> {
    vec![Part::Text(words.to_owned())]
}

fn command(id: &str, note: Option<String>, command: Template, timeout_s: u32) -> Result<GuestCommand> {
    Ok(GuestCommand {
        id: StepId::parse(id)?,
        note,
        command,
        timeout_s,
        failure: Failure::Fail,
    })
}

/// What runs inside a Docker host VM: the Compose file `file` of `env` (the `docker` target's),
/// run as root as `as_root` says.
pub(crate) fn guest(env: &Environment, file: &ComposeFile, as_root: AsRoot) -> Result<GuestCompose> {
    let members = classify(env, file)?;
    let plan = StartPlan::of(file);
    let project = Project::Named(file.name.clone());
    let folders: Vec<String> = host_folders::of_containers(env)
        .iter()
        .map(|f| format!("{}={} ", f.variable, f.in_vm))
        .collect();
    let vm = InVm {
        project: file.name.clone(),
        as_root,
        folders: folders.concat(),
    };
    let (runners, derived) = runners(env, &vm)?;
    let mut machines: Vec<Machine> = machines(env, file, &members, &project)?;
    for p in machines.iter_mut().flat_map(|m| m.ports.iter_mut()) {
        if let Some(publish) = &mut p.publish {
            publish.mode = PublishMode::Fixed;
            publish.address = None;
        }
    }
    Ok(GuestCompose {
        machines,
        networks: networks(env, |n| project.prefixed(n))?,
        slots: slots(env, &members, &plan, Starts::Guest)?,
        params: derived_switch(derived)?.into_iter().collect(),
        start: start(env, &vm, &plan, &members)?,
        ready: ready(&plan, &members, START)?,
        check_setup: stand_in_command(file, &vm, &STAND_INS_UP)?,
        check_teardown: stand_in_command(file, &vm, &STAND_INS_DOWN)?,
        status: command("ps", None, vm.status(), 60)?,
        runners,
    })
}

/// The start step's id.
const START: &str = "start";

/// The project's start ([`super::lifecycle::start`]), the project files Compose reads from its
/// environment read from the VM's copy first.
fn start(env: &Environment, vm: &InVm, plan: &StartPlan, members: &[Member]) -> Result<Vec<GuestCommand>> {
    let files = host_environment(env);
    lifecycle::start(env, plan, members, START, " in the VM")
        .into_iter()
        .map(|s| {
            let mut c = command(&s.id, Some(s.note), vm.compose(&files, &text(&s.args.join(" "))), s.timeout_s)?;
            if s.tolerated {
                c.failure = Failure::Tolerate;
            }
            Ok(c)
        })
        .collect()
}

/// Each position's runner (`compose run` of its `check` service, `ISOLOOM_DERIVED` passed on),
/// then a machine's own `exec` checks piped into it from the VM's copy; and the steps of the
/// runners that read `ISOLOOM_DERIVED`.
fn runners(env: &Environment, vm: &InVm) -> Result<(Vec<GuestRunner>, Vec<StepId>)> {
    let mut out = Vec::new();
    let mut derived = Vec::new();
    for p in planned(env) {
        let runner = compose_runner(vm, &p)?;
        derived.push(runner.command.id.clone());
        out.push(runner);
        if let Some((machine, execs)) = &p.execs {
            let piped = format!("exec -T {machine} sh -s < {OUTPUT_DIR}/docker/checks/{}", exec_runner(machine));
            out.push(GuestRunner {
                id: Name::parse(&p.exec_id())?,
                from: inside(machine),
                checks: execs.iter().map(|c| check(c)).collect(),
                command: command(&p.exec_step_id(), None, vm.compose("", &text(&piped)), timeout_s(execs))?,
                on_timeout: Vec::new(),
            });
        }
    }
    Ok((out, derived))
}

fn compose_runner(vm: &InVm, p: &Planned<'_>) -> Result<GuestRunner> {
    let run = [
        Part::Text(format!("--profile check run --rm --no-deps --env {DERIVED}=")),
        Part::Placeholder(Placeholder::Param(EnvName::parse(DERIVED)?)),
        Part::Text(format!(" {}", p.service)),
    ];
    // A runner cut off by its time limit leaves its one-off container behind.
    let sudo = vm.as_root.prefix();
    let leftover = format!(
        "{sudo}docker ps -aq --filter label=com.docker.compose.project={} --filter label=com.docker.compose.service={} --filter label=com.docker.compose.oneoff=True | xargs -r {sudo}docker rm -f",
        vm.project, p.service
    );
    Ok(GuestRunner {
        id: Name::parse(&p.service)?,
        from: p.pos.label(),
        checks: p.checks.iter().map(|c| check(c)).collect(),
        command: command(&p.step_id(), None, vm.compose("", &run), timeout_s(&p.checks))?,
        on_timeout: vec![command(&p.leftover_id(), None, Template::text(&leftover), 60)?],
    })
}

/// What is done to the access machine's stand-ins in the VM around the checks (`what`), when the
/// file has them.
fn stand_in_command(file: &ComposeFile, vm: &InVm, what: &StandInStep) -> Result<Vec<GuestCommand>> {
    let stand_ins = stand_ins(file);
    if stand_ins.is_empty() {
        return Ok(Vec::new());
    }
    let args = what.args(&stand_ins).join(" ");
    let mut command = command(what.id, what.note.map(str::to_owned), vm.compose("", &text(&args)), what.timeout_s)?;
    command.failure = what.failure;
    Ok(vec![command])
}

#[cfg(test)]
mod tests;
