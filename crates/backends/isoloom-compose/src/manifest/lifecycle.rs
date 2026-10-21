//! The operations of a Compose run, when it counts as ready, and how a start that tripped over a
//! previous run's leftovers is retried.

use isoloom_manifest::{Action, Argv, Condition, HostArgv, Lifecycle, NonEmpty, Park, Recovery, Retries, Step, StepId, Template, When};
use isoloom_plan::{Environment, Image, Lifetime, MachineShape};
use isoloom_render::run_manifest::{cleanup, noted, tolerated};

use super::machines::Member;
use super::{Project, compose, program};
use crate::error::Result;
use crate::start::StartPlan;

/// How long `up --wait` waits for the services to answer, once they started.
const WAIT_TIMEOUT: &str = "600";

/// A step running `argv` for at most `timeout_s`.
fn run(id: &str, argv: HostArgv, timeout_s: u32) -> Result<Step> {
    Ok(isoloom_render::run_manifest::run(id, argv, timeout_s)?)
}

/// A step running `action`.
fn act(id: &str, action: Action) -> Result<Step> {
    Ok(isoloom_render::run_manifest::act(id, action)?)
}

/// The id of the step that runs a leaf `init:` job.
pub(super) fn job_step(job: &str) -> String {
    format!("job-{job}")
}

/// The machine a job belongs to.
pub(super) fn owner<'a>(members: &'a [Member], job: &'a str) -> &'a str {
    members.iter().find(|m| m.name == job).and_then(|m| m.of.as_deref()).unwrap_or(job)
}

/// Up, down, park, resume and status, as Docker Compose commands and the driver's actions.
pub(super) fn lifecycle(env: &Environment, plan: &StartPlan, members: &[Member]) -> Result<Lifecycle> {
    let stop = || run("stop", compose(&["stop"])?, 300);
    Ok(Lifecycle {
        up: NonEmpty::new(up(env, plan, members)?, "lifecycle.up")?,
        down: NonEmpty::new(down(env)?, "lifecycle.down")?,
        park: Some(Park {
            pause: Some(vec![stop()?]),
            shutdown: Some(vec![stop()?]),
        }),
        resume: vec![run("start", compose(&["start"])?, 600)?],
        provision: None,
        seal: None,
        status: run("ps", compose(&["ps", "--all", "--format", "json"])?, 25)?,
    })
}

/// Pin the host ports, check they are free, clear stale networks, pull any published images,
/// start everything but the leaf jobs and wait for it, then run each leaf job attached.
fn up(env: &Environment, plan: &StartPlan, members: &[Member]) -> Result<Vec<Step>> {
    // A lab that publishes a built image (the build+image combo) pulls it first, so a student runs
    // the published image rather than building it; `up` then builds only what could not be pulled.
    // A lab that only builds locally keeps `--build`, so an author's edits are always rebuilt.
    let pull_published = publishes_a_built_image(env);
    let mut args = vec!["up", "--detach"];
    if !pull_published {
        args.push("--build");
    }
    args.extend(["--pull", "missing", "--wait", "--wait-timeout", WAIT_TIMEOUT]);
    args.extend(plan.wait.iter().map(String::as_str));
    let mut steps = vec![
        noted(act("pin-ports", Action::ComposePinPorts)?, "Picking the lab's host ports…"),
        act("ports-free", Action::ComposeCheckPortsFree)?,
        tolerated(act("stale-networks", Action::ComposeRemoveStaleNetworks)?),
    ];
    if pull_published {
        // Pull the published images; a service with no published image, or one not pushed yet,
        // fails the pull and is built by `up`. Tolerated, so an unreachable registry still falls
        // back to building rather than failing the run.
        let pull = ["pull", "--ignore-pull-failures"];
        steps.push(noted(tolerated(run("pull", compose(&pull)?, 1800)?), "Pulling the lab's published images…"));
    }
    steps.push(noted(run("up", compose(&args)?, 1800)?, "Starting the lab's containers…"));
    for job in &plan.jobs {
        let attached = ["up", "--no-deps", "--exit-code-from", job, job];
        let note = format!("Running {}'s set-up script…", owner(members, job));
        steps.push(noted(run(&job_step(job), compose(&attached)?, 600)?, &note));
    }
    Ok(steps)
}

/// Whether any machine runs a published image it also builds (the build+image combo), so the run
/// pulls before building and a student uses the published image instead of building it.
pub(super) fn publishes_a_built_image(env: &Environment) -> bool {
    env.machines
        .values()
        .any(|mp| matches!(&mp.image, Some(Image::Container { build: Some(_), published: true, .. })))
}

/// Remove everything, the check runners included; the volumes the spec keeps survive (`down`
/// without `--volumes`, then the others by name).
fn down(env: &Environment) -> Result<Vec<Step>> {
    let containers = || env.machines.values().filter(|m| m.shape == MachineShape::Container);
    let volumes = containers()
        .flat_map(|m| &m.volumes)
        .filter(|v| matches!(v.lifetime, Lifetime::Environment))
        .map(|v| v.key.as_str());
    let disks = containers().flat_map(|m| &m.disks).filter(|d| d.mount.is_some()).map(|d| d.key.as_str());
    let removable: Vec<&str> = volumes.chain(disks).collect();
    let kept = containers().flat_map(|m| &m.volumes).any(|v| matches!(v.lifetime, Lifetime::Persistent));
    let mut args = vec!["--profile", "check", "down"];
    if !kept {
        args.push("--volumes");
    }
    args.push("--remove-orphans");
    let mut down = run("down", compose(&args)?, 300)?;
    down.when = When::Always;
    let mut steps = vec![down];
    if kept && !removable.is_empty() {
        let mut rm: Vec<Template> = ["volume", "rm", "--force"].map(Template::text).into();
        rm.extend(dedup(removable).into_iter().map(|v| Project::Run.prefixed(v)));
        steps.push(cleanup(run("volumes", HostArgv::Every(Argv::new(program("docker")?, rm)), 120)?));
    }
    steps.push(cleanup(act("containers", Action::ComposeRemoveContainers)?));
    steps.push(cleanup(act("networks", Action::ComposeRemoveNetworks)?));
    Ok(steps)
}

fn dedup(mut keys: Vec<&str>) -> Vec<&str> {
    let mut seen = std::collections::BTreeSet::new();
    keys.retain(|k| seen.insert(*k));
    keys
}

/// The start step `up` succeeded (every long-running service answers), then each leaf job
/// exited 0.
pub(super) fn ready(plan: &StartPlan, members: &[Member], up: &str) -> Result<Vec<Condition>> {
    let up = Condition::StepOk {
        step: StepId::parse(up)?,
        means: Some("every long-running service is healthy (`--wait`: each port's probe, the route keepers, the machines' own healthchecks)".into()),
    };
    let jobs = plan.jobs.iter().map(|job| {
        Ok(Condition::StepOk {
            step: StepId::parse(&job_step(job))?,
            means: Some(format!("{}'s set-up script ({job}) exited 0", owner(members, job))),
        })
    });
    std::iter::once(Ok(up)).chain(jobs).collect()
}

/// A start that failed on a previous run's containers or networks (a name taken, a network left
/// with another label) clears them and tries once more.
pub(super) fn recovery() -> Result<Recovery> {
    Ok(Recovery {
        stale_markers: ["is already in use by container", "network with name", "has incorrect label"]
            .map(String::from)
            .into(),
        steps: vec![
            cleanup(act("containers", Action::ComposeRemoveContainers)?),
            cleanup(act("networks", Action::ComposeRemoveNetworks)?),
        ],
        retries: Retries::new(1)?,
        apply_retries: None,
    })
}
