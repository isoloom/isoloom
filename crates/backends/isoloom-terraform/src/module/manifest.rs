//! Helpers shared by the Terraform run-manifest builders (the clouds and the servers):
//! the small template and step-id utilities, a bare machine, a lab machine with its ports
//! fixed, and a state-read output. Each builder differs in its lifecycle, credentials and the
//! machines it makes; these are what they have in common, kept in one place so no copy drifts.

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::num::NonZeroU32;

use isoloom_manifest::{
    Action, Argv, Delivery, EnvName, Error as ManifestError, GoneError, HostArgv, Identity, Machine, Name, NonEmpty, OsName, Output, OutputRole, OutputSource,
    Param, Part, Placeholder, Program, PublishMode, RelPath, Role, RunVar, SetBy, SnakeName, StateOwner, Step, StepId, Template, TerraformIdentity, TfVarShape,
};
use isoloom_plan::{Environment, MachinePlan, Vm};
use isoloom_render::run_manifest::{addresses, arch, noted, port, run};

use crate::module::variable::Variable;

/// How long a run lives when the embedder sets nothing, in minutes: the reaper's.
pub(crate) const DEFAULT_LIFE_MIN: u32 = 240;

/// How long a machine's set-up may take once it can run (after the apply, the project in).
pub(crate) const SETUP_S: u32 = 2400;

/// How long the controller's playbooks may take.
pub(crate) const PLAYBOOKS_S: u32 = 7200;

/// How long a machine may stay out of reach over SSH, once it answered, before the run fails.
pub(crate) const UNREACHABLE_S: u32 = 600;

/// How long `terraform init` may take: its providers downloaded on a slow link.
const INIT_S: u32 = 900;

/// How long a `terraform apply` may take.
const APPLY_S: u32 = 3600;

/// A `terraform` step running `args` (`init`'s last one names the runner's state): its data
/// folder in the run's state folder, no prompt, and `vars`, the values the runner gives the
/// module (`TF_VAR_<name>`: its key, its slot on a server).
pub(crate) fn terraform(step: &str, args: Vec<Template>, timeout: u32, note: &str, vars: &[(&str, RunVar)]) -> Result<Step, ManifestError> {
    let command = HostArgv::Every(Argv::new(Program::parse("terraform")?, args));
    let mut s = noted(run(step, command, timeout)?, note);
    s.env = terraform_env(vars.iter().map(|(name, var)| (*name, Template::from(Placeholder::Run(*var)))).collect())?;
    Ok(s)
}

/// The module in place, its state the runner's, `gone` its providers' errors for a resource
/// already gone (`identity.terraform.gone`).
pub(crate) fn runner_identity(gone: Vec<GoneError>) -> Result<Identity, ManifestError> {
    Ok(Identity::Terraform(TerraformIdentity {
        module: RelPath::parse(".")?,
        state: StateOwner::Runner,
        var_files: Vec::new(),
        gone,
    }))
}

/// A stale Terraform state lock cleared (a crashed run's).
pub(crate) fn clear_lock() -> Result<Step, ManifestError> {
    Ok(Step::action(id("clear-lock")?, Action::TerraformClearLock))
}

/// `init`, naming the runner's state, with `vars` ([`terraform`]).
pub(crate) fn init(vars: &[(&str, RunVar)]) -> Result<Step, ManifestError> {
    let mut args = words(&["init", "-input=false", "-no-color"]);
    args.push(run_var("-backend-config=path=", RunVar::StateDir, "/terraform.tfstate"));
    terraform("init", args, INIT_S, "Preparing Terraform and its providers…", vars)
}

/// `apply`, under step id `step`, saying `note`, with `vars` ([`terraform`]).
pub(crate) fn apply(step: &str, note: &str, vars: &[(&str, RunVar)]) -> Result<Step, ManifestError> {
    terraform(step, words(&["apply", "-auto-approve", "-input=false", "-no-color"]), APPLY_S, note, vars)
}

/// A step id.
pub(crate) fn id(text: &str) -> Result<StepId, ManifestError> {
    StepId::parse(text)
}

/// A timeout as a number of seconds, `None` when zero (no timeout).
pub(crate) fn secs(n: u32) -> Option<NonZeroU32> {
    NonZeroU32::new(n)
}

/// `${run.<var>}` within text.
pub(crate) fn run_var(before: &str, var: RunVar, after: &str) -> Template {
    Template::from_parts([Part::Text(before.into()), Part::Placeholder(Placeholder::Run(var)), Part::Text(after.into())])
}

/// `${outputs.<name>}`.
pub(crate) fn output(name: &str) -> Result<Template, ManifestError> {
    Ok(Template::from(Placeholder::Output(SnakeName::parse(name)?)))
}

/// `words` as templates, each plain text.
pub(crate) fn words(words: &[&str]) -> Vec<Template> {
    words.iter().map(|w| Template::text(w)).collect()
}

/// A `terraform` step's environment: its data folder in the run's state, automation on and no
/// prompt, then `extra` (the run-time values the runner gives the module, such as its key).
pub(crate) fn terraform_env(extra: Vec<(&str, Template)>) -> Result<BTreeMap<EnvName, Template>, ManifestError> {
    let mut all = vec![
        ("TF_DATA_DIR", run_var("", RunVar::StateDir, "/.terraform")),
        ("TF_IN_AUTOMATION", Template::text("1")),
        ("TF_INPUT", Template::text("0")),
    ];
    all.extend(extra);
    all.into_iter().map(|(k, v)| Ok((EnvName::parse(k)?, v))).collect()
}

/// A bare machine on its `addresses`: no OS, resources, ports or volumes set.
pub(crate) fn machine(name: &str, role: Role, addresses: BTreeMap<Name, IpAddr>) -> Result<Machine, ManifestError> {
    let mut m = isoloom_render::run_manifest::machine(name, role)?;
    m.addresses = addresses;
    Ok(m)
}

/// A lab machine (role `member`, or `access`): a bare machine with its OS, Windows flag and
/// architecture, then each spec port fixed at its own number on `at` (the machine's public
/// address).
pub(crate) fn lab_machine(name: &str, mp: &MachinePlan, spec: &isoloom_spec::model::Machine, at: &Template) -> Result<Machine, ManifestError> {
    // The access machine keeps its role (where the user works from); every machine is made at `up`.
    let role = if spec.access { Role::Access } else { Role::Member };
    let mut m = machine(name, role, addresses(mp)?)?;
    // A VM's OS; a container (an LXC one on Proxmox) runs its image's.
    m.os = Vm::of(name, mp).map(|vm| OsName::parse(vm.os)).transpose()?;
    m.windows = mp.is_windows();
    m.arch = Some(arch(mp.arch));
    m.resources = Some(isoloom_render::run_manifest::resources(mp));
    for p in &spec.ports {
        let mut port = port(&spec.port_label(p), p, &mp.ports, at)?;
        if let Some(publish) = port.publish.as_mut() {
            publish.mode = PublishMode::Fixed;
        }
        m.ports.push(port);
    }
    Ok(m)
}

/// The Terraform variable `name`, a string.
pub(crate) fn tf_var_string(name: &str) -> Result<NonEmpty<Delivery>, ManifestError> {
    Ok(NonEmpty::one(Delivery::TfVar {
        var: SnakeName::parse(name)?,
        shape: TfVarShape::String,
    }))
}

/// Variable `v` as a param the embedder gives: required when it has no default (or `required`), a
/// `bool` taking `true` or `false`.
pub(crate) fn param(v: &Variable, required: bool) -> Result<Param, ManifestError> {
    Ok(Param {
        name: EnvName::parse(v.name())?,
        description: v.description().map_or_else(|| format!("The module's `{}`.", v.name()), |d| format!("{d}.")),
        default: v.default_text(),
        values: if v.is_bool() { vec!["true".into(), "false".into()] } else { Vec::new() },
        required: required || !v.has_default(),
        hosts: BTreeSet::new(),
        providers: BTreeSet::new(),
        set_by: SetBy::Embedder,
        deliver: tf_var_string(v.name())?,
    })
}

/// A Terraform state output `name` (role `role`), read by the runner as `${outputs.<name>}`.
///
/// # Errors
///
/// Fails on a name the manifest refuses.
pub(crate) fn terraform_output(name: &str, role: OutputRole, description: &str) -> Result<Output, ManifestError> {
    Ok(Output {
        name: SnakeName::parse(name)?,
        description: Some(description.to_owned()),
        role,
        sensitive: false,
        format: None,
        from: OutputSource::TerraformOutput {
            name: isoloom_manifest::Identifier::parse(name)?,
            default: None,
        },
    })
}

/// The seal's run-manifest param (`crate::cloud::vm::seal`): the runner sets it `true` once the
/// machines are set up (its default is `false`) and records it with the run. It is delivered as
/// the `isoloom_sealed` Terraform variable: a cloud module's rules switch on it; a server's module
/// (Proxmox's, ESXi's) has no such variable (its router is sealed by the seal's SSH step), so
/// Terraform ignores it there. `closed_at` names where the internet is closed for the description
/// (the clouds' "network", a server's "router").
pub(crate) fn seal_param(name: &str, closed_at: &str) -> Result<Param, ManifestError> {
    Ok(Param {
        name: EnvName::parse(name)?,
        description: format!("true once the machines are set up: the internet is closed to the `internet: false` networks, at the {closed_at}."),
        default: Some("false".into()),
        values: vec!["true".into(), "false".into()],
        required: false,
        hosts: BTreeSet::new(),
        providers: BTreeSet::new(),
        set_by: SetBy::Runner,
        deliver: tf_var_string(name)?,
    })
}

/// The VMs that read the launch-time input `name`, in start order (empty: none do).
pub(crate) fn input_readers(env: &Environment, name: &str) -> Result<Vec<Name>, ManifestError> {
    env.vms()
        .filter(|Vm { plan: mp, .. }| mp.inputs.iter().any(|i| i == name))
        .map(|Vm { name: n, .. }| Name::parse(n))
        .collect()
}

/// A check runner per position, on the machine `crate::machine::check_machine` says, its step
/// from `step` (given the position, that machine and its checks; `None`: no runner there): the
/// one loop of every Terraform target's run manifest.
pub(crate) fn check_runners(
    env: &Environment,
    step: impl Fn(&isoloom_plan::checks::Position, &str, &[&isoloom_plan::checks::Resolved]) -> Result<Option<isoloom_manifest::Step>, ManifestError>,
) -> Result<Vec<isoloom_manifest::CheckRunner>, ManifestError> {
    let mut out = Vec::new();
    for (pos, group) in isoloom_plan::checks::by_position(env.spec(), &env.checks) {
        let Some(machine) = crate::machine::check_machine(env, &pos) else { continue };
        let Some(step) = step(&pos, machine, &group)? else { continue };
        out.push(isoloom_manifest::CheckRunner {
            id: Name::parse(pos.id())?,
            from: format!("from {machine}"),
            checks: group.iter().map(|c| isoloom_render::run_manifest::check(c)).collect(),
            step,
            on_timeout: Vec::new(),
        });
    }
    Ok(out)
}

/// The ready file (`isoloom-ready/1`) of the machine `user@host`, through `via` when it is
/// behind one, read every 10 s until `within` seconds: its path the module's `ready_file`
/// output, `log` the file of the set-up's output whose tail explains a failed step, when the
/// set-up writes one. The one condition every Terraform target's machines are ready by.
pub(crate) fn ready_file(
    host: Template,
    user: &str,
    log: Option<&str>,
    via: Option<Template>,
    within: u32,
) -> Result<isoloom_manifest::Condition, ManifestError> {
    let unset = || ManifestError::OutOfRange {
        what: "a timeout",
        value: 0,
        min: 1,
        max: u32::MAX.into(),
    };
    Ok(isoloom_manifest::Condition::ReadyFile {
        host,
        user: Template::text(user),
        path: output("ready_file")?,
        log: log.map(Template::text),
        grammar: isoloom_manifest::ReadyGrammar::V1,
        via,
        timeout_s: secs(within).ok_or_else(unset)?,
        unreachable_timeout_s: secs(UNREACHABLE_S),
        interval_s: secs(10).ok_or_else(unset)?,
    })
}
