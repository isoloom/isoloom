//! Compose inside a VM (`docker-vm`): the `docker` target's Compose file run from the VM's copy of
//! the project, by shell commands the VM's backend runs through its own way in. The same services,
//! roles, slots, start plan and check runners as on this computer's Docker ([`super::docker`]),
//! with what differs in a VM: the file's own project name (one lab per VM), every published port
//! fixed at its `publish` number (the VM forwards it, or answers at its own address), the project
//! files Compose reads from its environment read from the VM's copy, and root's Docker (`sudo`,
//! keeping the values the way in forwarded).

use isoloom_manifest::{EnvName, Machine, Name, Part, Placeholder, PublishMode, StepId, Template};
use isoloom_plan::Environment;
use isoloom_render::container::{exec_runner, host_environment};
use isoloom_render::run_manifest::guest::{GuestCommand, GuestCompose, GuestRunner};
use isoloom_render::run_manifest::{DERIVED, check, derived_switch, networks, timeout_s};
use isoloom_render::{OUTPUT_DIR, steps};

use super::Project;
use super::checks::{Planned, planned, stand_ins};
use super::lifecycle::{job_step, owner, publishes_a_built_image, ready};
use super::machines::{Member, classify, machines};
use super::slots::{Starts, slots};
use crate::error::Result;
use crate::file::ComposeFile;
use crate::start::StartPlan;

/// How long `up --wait` waits for the services to answer, once they started.
const WAIT_TIMEOUT: &str = "600";

/// The commands of a project in the VM: `docker compose` as root under its name, with its file
/// and its ports published on every address of the VM at their own numbers.
struct InVm {
    project: String,
}

impl InVm {
    /// `cd <project copy> && <before>sudo -E … docker compose … <rest>`, `-E` keeping what the way
    /// in forwarded and `before` exported.
    fn compose(&self, before: &str, rest: &[Part]) -> Template {
        let head = format!(
            "cd {} && {before}sudo -E env ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 ISOLOOM_PUBLISH_FIXED=1 docker compose --project-name {} --file {OUTPUT_DIR}/docker/compose.yml ",
            steps::PROJECT_DIR,
            self.project
        );
        Template::from_parts(std::iter::once(Part::Text(head)).chain(rest.iter().cloned()))
    }

    /// The containers, as `ps` lists them: nothing before Docker is in the VM.
    fn status(&self) -> Template {
        Template::text(&format!(
            "if command -v docker >/dev/null; then sudo docker compose --project-name {} ps --all --format json; fi",
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
    })
}

/// What runs inside `docker-vm`'s VM: the Compose file `file` of `env` (the `docker` target's).
pub(crate) fn guest(env: &Environment, file: &ComposeFile) -> Result<GuestCompose> {
    let members = classify(env, file)?;
    let plan = StartPlan::of(file);
    let project = Project::Named(file.name.clone());
    let vm = InVm { project: file.name.clone() };
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
        check_setup: check_setup(file, &vm)?,
        status: command("ps", None, vm.status(), 60)?,
        runners,
    })
}

/// The start step's id.
const START: &str = "start";

/// Everything but the leaf jobs up and healthy, then each leaf job attached; the project files
/// Compose reads from its environment read from the VM's copy first.
fn start(env: &Environment, vm: &InVm, plan: &StartPlan, members: &[Member]) -> Result<Vec<GuestCommand>> {
    let files = host_environment(env);
    // As on the host: a lab that publishes a built image pulls it first (a student runs the
    // published image), so it drops `--build` and `up` builds only what could not be pulled; a
    // lab that only builds locally keeps `--build`.
    let pull_published = publishes_a_built_image(env);
    let build = if pull_published { "" } else { " --build" };
    let mut up = format!("up --detach{build} --pull missing --wait --wait-timeout {WAIT_TIMEOUT}");
    for service in &plan.wait {
        up.push(' ');
        up.push_str(service);
    }
    let mut out = Vec::new();
    if pull_published {
        let pulling = Some("Pulling the lab's published images in the VM…".to_owned());
        let pull = text("pull --ignore-pull-failures");
        out.push(command("pull", pulling, vm.compose(&files, &pull), 1800)?);
    }
    let starting = Some("Starting the lab's containers in the VM…".to_owned());
    out.push(command(START, starting, vm.compose(&files, &text(&up)), 1800)?);
    for job in &plan.jobs {
        let note = Some(format!("Running {}'s set-up script…", owner(members, job)));
        let attached = format!("up --no-deps --exit-code-from {job} {job}");
        out.push(command(&job_step(job), note, vm.compose(&files, &text(&attached)), 600)?);
    }
    Ok(out)
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
            let id = format!("check-exec-{}", p.pos.id());
            let piped = format!("exec -T {machine} sh -s < {OUTPUT_DIR}/docker/checks/{}", exec_runner(machine));
            out.push(GuestRunner {
                id: Name::parse(&format!("exec-{}", p.pos.id()))?,
                from: format!("inside {machine}"),
                checks: execs.iter().map(|c| check(c)).collect(),
                command: command(&id, None, vm.compose("", &text(&piped)), timeout_s(execs))?,
                on_timeout: Vec::new(),
            });
        }
    }
    Ok((out, derived))
}

fn compose_runner(vm: &InVm, p: &Planned<'_>) -> Result<GuestRunner> {
    let id = format!("check-{}", p.pos.id());
    let run = [
        Part::Text("--profile check run --rm --no-deps --env ISOLOOM_DERIVED=".into()),
        Part::Placeholder(Placeholder::Param(EnvName::parse(DERIVED)?)),
        Part::Text(format!(" {}", p.service)),
    ];
    // A runner cut off by its time limit leaves its one-off container behind.
    let leftover = format!(
        "sudo docker ps -aq --filter label=com.docker.compose.project={} --filter label=com.docker.compose.service={} --filter label=com.docker.compose.oneoff=True | xargs -r sudo docker rm -f",
        vm.project, p.service
    );
    Ok(GuestRunner {
        id: Name::parse(&p.service)?,
        from: p.pos.label(),
        checks: p.checks.iter().map(|c| check(c)).collect(),
        command: command(&id, None, vm.compose("", &run), timeout_s(&p.checks))?,
        on_timeout: vec![command(&format!("rm-{id}"), None, Template::text(&leftover), 60)?],
    })
}

/// The access machine's stand-ins, when the file has them.
fn check_setup(file: &ComposeFile, vm: &InVm) -> Result<Vec<GuestCommand>> {
    let stand_ins = stand_ins(file);
    if stand_ins.is_empty() {
        return Ok(Vec::new());
    }
    let up = format!("--profile check up --detach --wait --no-deps {}", stand_ins.join(" "));
    let note = Some("Starting the access machine's stand-in…".to_owned());
    Ok(vec![command("stand-ins", note, vm.compose("", &text(&up)), 600)?])
}

#[cfg(test)]
mod tests;
