//! What a Proxmox run does: Terraform's `init` then `apply` (and `destroy`), each with the run's
//! own values in its environment; when it counts as ready (the apply, then each Linux machine's
//! ready file, read through the router); and its check runners, each piped to its machine over
//! SSH through the router.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use isoloom_manifest::{
    Action, Argv, CheckRunner, Condition, EnvName, HostArgv, Lifecycle, ManifestError, Name, NonEmpty, Part, Placeholder, Program, ReadyGrammar, RelPath, Run,
    RunVar, SnakeName, Step, StepId, Template,
};
use isoloom_plan::Environment;
use isoloom_plan::checks::by_position;
use isoloom_render::run_manifest::{check, noted, run, timeout_s};

use crate::machine::{controller, vms};
use crate::server::router::check_host;

/// The user every Linux VM and the router let in with the runner's key.
const USER: &str = "isoloom";

/// Where cloud-init writes the set-up's output: its tail explains a failed step.
const SETUP_LOG: &str = "/var/log/cloud-init-output.log";

/// How long a machine's set-up may take after the apply, and its controller's playbooks.
const SETUP_S: u32 = 2400;
const PLAYBOOKS_S: u32 = 7200;

/// How long a machine may stay out of reach over SSH before the run fails.
const UNREACHABLE_S: u32 = 600;

fn id(text: &str) -> Result<StepId, ManifestError> {
    StepId::parse(text)
}

fn secs(n: u32) -> Option<NonZeroU32> {
    NonZeroU32::new(n)
}

/// `${run.<var>}` and `${outputs.<name>}` within text.
fn run_var(before: &str, var: RunVar, after: &str) -> Template {
    Template::from_parts([Part::Text(before.into()), Part::Placeholder(Placeholder::Run(var)), Part::Text(after.into())])
}

fn output(name: &str) -> Result<Template, ManifestError> {
    Ok(Template::from(Placeholder::Output(SnakeName::parse(name)?)))
}

/// A `terraform` step running `args` (`init`'s last one names the runner's state): its data
/// folder in the run's state folder, no prompt, and the values the runner gives the module (its
/// slot on the server, its key).
fn terraform(step: &str, args: Vec<Template>, timeout: u32, note: &str) -> Result<Step, ManifestError> {
    let command = HostArgv::Every(Argv::new(Program::parse("terraform")?, args));
    let mut s = noted(run(step, command, timeout)?, note);
    let env: [(&str, Template); 6] = [
        ("TF_DATA_DIR", run_var("", RunVar::StateDir, "/.terraform")),
        ("TF_IN_AUTOMATION", Template::text("1")),
        ("TF_INPUT", Template::text("0")),
        ("TF_VAR_slot", Template::from(Placeholder::Run(RunVar::Slot))),
        ("TF_VAR_ssh_public_key", Template::from(Placeholder::Run(RunVar::SshPublicKey))),
        ("TF_VAR_proxmox_ssh_private_key_file", Template::from(Placeholder::Run(RunVar::SshKey))),
    ];
    s.env = env
        .into_iter()
        .map(|(k, v)| Ok((EnvName::parse(k)?, v)))
        .collect::<Result<BTreeMap<_, _>, ManifestError>>()?;
    Ok(s)
}

/// `words` as templates, each plain text.
fn words(words: &[&str]) -> Vec<Template> {
    words.iter().map(|w| Template::text(w)).collect()
}

/// A stale lock of a crashed run cleared, then `init`, then `apply` (or `destroy`).
pub(super) fn lifecycle() -> Result<Lifecycle, ManifestError> {
    let lock = || Ok::<Step, ManifestError>(Step::action(id("clear-lock")?, Action::TerraformClearLock));
    let init = || {
        let mut args = words(&["init", "-input=false", "-no-color"]);
        args.push(run_var("-backend-config=path=", RunVar::StateDir, "/terraform.tfstate"));
        terraform("init", args, 600, "Preparing Terraform and its providers…")
    };
    let apply = terraform(
        "apply",
        words(&["apply", "-auto-approve", "-input=false", "-no-color"]),
        3600,
        "Creating the lab's VMs on the server…",
    )?;
    // No refresh: the state names everything to remove, and a router whose guest agent never
    // ran would hold a refresh for the agent's whole wait.
    let destroy = terraform(
        "destroy",
        words(&["destroy", "-auto-approve", "-input=false", "-no-color", "-refresh=false"]),
        1800,
        "Removing the lab from the server…",
    )?;
    Ok(Lifecycle {
        up: NonEmpty::new(vec![lock()?, init()?, apply], "lifecycle.up")?,
        down: NonEmpty::new(vec![lock()?, init()?, destroy], "lifecycle.down")?,
        park: None,
        resume: Vec::new(),
        provision: None,
        status: Step::action(id("state")?, Action::TerraformReadState),
    })
}

/// The apply succeeded, then each Linux VM's set-up and the controller's playbooks are done.
pub(super) fn ready(env: &Environment) -> Result<Vec<Condition>, ManifestError> {
    let mut out = vec![Condition::StepOk {
        step: id("apply")?,
        means: Some("the VMs exist and booted; each sets itself up from its cloud-init".into()),
    }];
    for (_, mp) in vms(env).filter(|(_, mp)| !mp.is_windows()) {
        if let Some(a) = mp.addresses.values().next() {
            out.push(ready_file(&a.to_string(), SETUP_S)?);
        }
    }
    if controller::needed(env) {
        let first = env.networks.values().next().and_then(|n| isoloom_spec::validate::Cidr::parse(&n.cidr));
        if let Some(c) = first {
            out.push(ready_file(&c.controller().to_string(), PLAYBOOKS_S)?);
        }
    }
    Ok(out)
}

/// The ready file of the machine at `host`, read through the router.
fn ready_file(host: &str, within: u32) -> Result<Condition, ManifestError> {
    let unset = || ManifestError::OutOfRange {
        what: "a timeout",
        value: 0,
        min: 1,
        max: u32::MAX.into(),
    };
    Ok(Condition::ReadyFile {
        host: Template::text(host),
        user: Template::text(USER),
        path: output("ready_file")?,
        log: Some(Template::text(SETUP_LOG)),
        grammar: ReadyGrammar::V1,
        via: Some(through_router()?),
        timeout_s: secs(within).ok_or_else(unset)?,
        unreachable_timeout_s: secs(UNREACHABLE_S),
        interval_s: secs(10).ok_or_else(unset)?,
    })
}

/// `isoloom@${outputs.ip}`: the router, the machines' jump host.
fn through_router() -> Result<Template, ManifestError> {
    Ok(Template::from_parts([
        Part::Text(format!("{USER}@")),
        Part::Placeholder(Placeholder::Output(SnakeName::parse("ip")?)),
    ]))
}

/// A check runner per position: its script piped to its machine's `sh -s` through the router,
/// the derived checks' switch in its environment, for its checks' waits plus a margin.
pub(super) fn runners(env: &Environment) -> Result<Vec<CheckRunner>, ManifestError> {
    let mut out = Vec::new();
    for (pos, group) in by_position(env.spec(), &env.checks) {
        let Some((machine, host)) = check_host(env, &pos) else { continue };
        let derived = Template::from_parts([
            Part::Text("ISOLOOM_DERIVED=".into()),
            Part::Placeholder(Placeholder::Param(EnvName::parse("ISOLOOM_DERIVED")?)),
            Part::Text(" sh -s".into()),
        ]);
        let with = [
            ("host", Template::text(&host)),
            ("user", Template::text(USER)),
            ("via", through_router()?),
            ("command", derived),
        ];
        let mut step = Step::action(id(&format!("check-{}", pos.id()))?, Action::SshRun);
        step.run = Run::Action {
            action: Action::SshRun,
            with: with
                .into_iter()
                .map(|(k, v)| Ok((SnakeName::parse(k)?, v)))
                .collect::<Result<_, ManifestError>>()?,
        };
        step.stdin = Some(RelPath::parse(&format!("checks/{}.sh", pos.id()))?);
        step.timeout_s = secs(timeout_s(&group));
        out.push(CheckRunner {
            id: Name::parse(pos.id())?,
            from: format!("from {machine}"),
            checks: group.iter().map(|c| check(c)).collect(),
            step,
            on_timeout: Vec::new(),
        });
    }
    Ok(out)
}
