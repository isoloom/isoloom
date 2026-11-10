//! What a Compose run is given at run time: the params the Compose file reads (where published
//! ports listen, whether their host ports are fixed, whether derived checks run) and the slots
//! of the values it must never hold (inputs, secrets, project files), each delivered to the
//! steps that create or start the containers using it.

use std::collections::BTreeSet;

use isoloom_manifest::{
    Delivery, EnvName, Identifier, Machine, Name, NonEmpty, Param, PublishMode, Role, SecretSlot, SecretSource, SetBy, SlotKind, StepId, StepSelection, Store,
};
use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::model::secret_variable;

use isoloom_render::container::project_variables;
use isoloom_render::run_manifest::held_secrets;

use super::lifecycle::{job_step, owner};
use super::machines::Member;
use crate::error::Result;
use crate::start::StartPlan;

/// The host address published ports listen on; set: each published port takes its `publish`
/// number.
use isoloom_manifest::contract::{PUBLISH_ADDRESS, PUBLISH_FIXED};
/// `0`: the checks derived from the spec are skipped.
use isoloom_render::run_manifest::DERIVED;

fn env_name(name: &str) -> Result<EnvName> {
    Ok(EnvName::parse(name)?)
}

/// An environment variable of `steps`.
fn env(var: &str, steps: StepSelection) -> Result<NonEmpty<Delivery>> {
    Ok(NonEmpty::one(Delivery::Env { var: env_name(var)?, steps }))
}

fn param(name: &str, description: &str, default: Option<&str>, values: &[&str], steps: StepSelection) -> Result<Param> {
    Ok(Param {
        name: env_name(name)?,
        description: description.into(),
        default: default.map(String::from),
        values: values.iter().map(|v| (*v).to_owned()).collect(),
        required: false,
        hosts: BTreeSet::new(),
        providers: BTreeSet::new(),
        set_by: SetBy::Embedder,
        deliver: env(name, steps)?,
    })
}

/// The params the Compose file reads: the publish address and the fixed host ports when a port
/// is published, the derived checks' switch when a runner reads it.
pub(super) fn params(machines: &[Machine], derived: &[StepId]) -> Result<Vec<Param>> {
    let published: Vec<PublishMode> = machines
        .iter()
        .flat_map(|m| &m.ports)
        .filter_map(|p| p.publish.as_ref().map(|p| p.mode))
        .collect();
    let mut out = Vec::new();
    if !published.is_empty() {
        out.push(param(
            PUBLISH_ADDRESS,
            "The host address published ports listen on.",
            Some("127.0.0.1"),
            &[],
            StepSelection::Every,
        )?);
        // Declared whenever a port is published, even if every one is already `fixed:` (no
        // ephemeral one to pin): the program running the lab (to keep the lab's own ports) may
        // pass it for any lab that publishes a port, and it is a no-op where the compose file
        // already pins the port to its `publish:` number.
        let what = "Set: each published port takes its `publish:` number instead of an ephemeral one.";
        out.push(param(PUBLISH_FIXED, what, None, &["1"], StepSelection::Every)?);
    }
    if let Ok(steps) = NonEmpty::new(derived.to_vec(), "the check steps") {
        let what = "0: the checks derived from the spec (reachability, no internet) are skipped.";
        out.push(param(DERIVED, what, Some("1"), &["0", "1"], StepSelection::Only(steps))?);
    }
    Ok(out)
}

/// Where Compose runs, and so which steps create or start the containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Starts {
    /// On this computer's Docker: `up`, `start` (a resume) and the leaf jobs; the runner reads the
    /// project files Compose takes from its environment.
    Host,
    /// Inside a VM: its `start` and the leaf jobs (a resume brings the VM back as it was); the
    /// project files are read in the VM, from its copy of the project.
    Guest,
}

impl Starts {
    /// The steps that create or start containers, before the leaf jobs.
    fn steps(self) -> &'static [&'static str] {
        match self {
            Starts::Host => &["up", "start"],
            Starts::Guest => &["start"],
        }
    }
}

/// The slots: each input a container reads, each secret a container gets, and on the host each
/// project file Compose takes from its environment.
pub(super) fn slots(env: &Environment, members: &[Member], plan: &StartPlan, starts: Starts) -> Result<Vec<SecretSlot>> {
    let containers: Vec<(&String, &isoloom_plan::MachinePlan)> = env.machines.iter().filter(|(_, m)| m.shape == MachineShape::Container).collect();
    let mut out = Vec::new();
    for input in &env.inputs {
        // A machine reads an input from its environment, or from a file Compose puts in place.
        let readers: Vec<&str> = containers.iter().filter(|(_, m)| m.reads_input(&input.name)).map(|(n, _)| n.as_str()).collect();
        if !readers.is_empty() {
            let mut s = slot(&input.name, SlotKind::Input, None, &input.name, reach(&readers, members, plan, starts)?)?;
            s.sensitive = input.sensitive;
            // An input may be left out, but Compose won't create or start a container whose
            // secret's variable is unset: the runner delivers it empty instead (Compose keeps the
            // file, empty).
            s.default = Some(String::new());
            out.push(s);
        }
    }
    for held in held_secrets(env, MachineShape::Container) {
        let var = secret_variable(held.name);
        out.push(slot(
            held.name,
            SlotKind::Secret,
            Some(held.source),
            &var,
            reach(&held.holders, members, plan, starts)?,
        )?);
    }
    if starts == Starts::Guest {
        return Ok(out);
    }
    for (var, path) in project_variables(env) {
        let source = SecretSource {
            store: Store::File,
            reference: path,
        };
        out.push(slot(&var, SlotKind::Secret, Some(source), &var, reach(&[], members, plan, starts)?)?);
    }
    Ok(out)
}

/// Who gets a value: some machines and their `init:` jobs, through the steps that create or
/// start them ([`Starts`]): on the host `up` and `start` (resume: Compose writes a secret's file
/// again from its variable each time it starts the container, and refuses to start without it;
/// Compose 5.5.1), and the leaf jobs run on their own.
struct Reach {
    machines: Vec<Name>,
    steps: StepSelection,
}

fn reach(machines: &[&str], members: &[Member], plan: &StartPlan, starts: Starts) -> Result<Reach> {
    let jobs = members
        .iter()
        .filter(|m| m.role == Role::Job && m.of.as_deref().is_some_and(|o| machines.contains(&o)));
    let names = machines.iter().copied().chain(jobs.map(|j| j.name.as_str()));
    let leaf_jobs = plan.jobs.iter().filter(|j| machines.contains(&owner(members, j))).map(|j| job_step(j));
    let steps = starts.steps().iter().map(|s| (*s).to_owned()).chain(leaf_jobs);
    Ok(Reach {
        machines: names.map(Name::parse).collect::<Result<_, _>>()?,
        steps: StepSelection::Only(NonEmpty::new(steps.map(|s| StepId::parse(&s)).collect::<Result<_, _>>()?, "the steps")?),
    })
}

/// A sensitive slot named `name`, delivered as the variable `var`; an input isn't required (its
/// slot says what to deliver without a value), a secret or a project file is.
fn slot(name: &str, kind: SlotKind, source: Option<SecretSource>, var: &str, reach: Reach) -> Result<SecretSlot> {
    Ok(SecretSlot {
        name: Identifier::parse(name)?,
        kind,
        required: kind != SlotKind::Input,
        sensitive: true,
        machines: reach.machines,
        providers: BTreeSet::new(),
        source,
        default: None,
        deliver: env(var, reach.steps)?,
    })
}
