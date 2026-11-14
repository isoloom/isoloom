//! What a server's run does: Terraform's `init` then `apply` (and `destroy`), each with the run's
//! own values in its environment; when it counts as ready (the apply, then each Linux machine's
//! ready file, read through the router); its seal; and its check runners, each piped to its
//! machine over SSH through the router.

use isoloom_manifest::{
    Action, CheckRunner, Condition, EnvName, Error as ManifestError, Lifecycle, NonEmpty, Part, PathTemplate, Placeholder, RelPath, RunVar, SnakeName, Step,
    Template,
};
use isoloom_plan::{Environment, Vm};
use isoloom_render::run_manifest::timeout_s;

use crate::machine::controller;
use crate::module::manifest::{self, PLAYBOOKS_S, SETUP_S, id, secs, terraform, words};
use crate::server::router::check_host;
use crate::ssh::Login;

use crate::machine::USER;

/// The module's variables the runner sets itself, each from one of its own values: its slot on
/// the server, its key's public half (the VMs let it in), and its private half's file, under the
/// name Terraform signs in to the machines with (`ssh_private_key_file`) and under `private_key`
/// (the module's name for the file it reaches the server with, when that is another). Never
/// params an embedder gives.
pub(crate) fn runner_set(private_key: &str) -> Vec<(&str, RunVar)> {
    let mut out = vec![
        ("slot", RunVar::Slot),
        ("ssh_public_key", RunVar::SshPublicKey),
        ("ssh_private_key_file", RunVar::SshKey),
    ];
    if private_key != "ssh_private_key_file" {
        out.push((private_key, RunVar::SshKey));
    }
    out
}

/// A stale lock of a crashed run cleared, then `init`, then `apply` (or `destroy`). The apply
/// ends once the machines are set up and the networks without internet are sealed (the module's
/// own waits and seal, `crate::ready`, `crate::server::router::seal`). Each Terraform step has
/// the runner's own values in its environment ([`runner_set`], `private_key` the module's
/// variable for its key's file).
pub(crate) fn lifecycle(private_key: &str) -> Result<Lifecycle, ManifestError> {
    let named: Vec<(String, RunVar)> = runner_set(private_key).iter().map(|(v, run)| (format!("TF_VAR_{v}"), *run)).collect();
    let vars: Vec<(&str, RunVar)> = named.iter().map(|(v, run)| (v.as_str(), *run)).collect();
    let vars = vars.as_slice();
    let (lock, init) = (manifest::clear_lock, || manifest::init(vars));
    let apply = |step, note| manifest::apply(step, note, vars);
    // No refresh: the state names everything to remove, and a router whose guest agent never
    // ran would hold a refresh for the agent's whole wait.
    let destroy = terraform(
        "destroy",
        words(&["destroy", "-auto-approve", "-input=false", "-no-color", "-refresh=false"]),
        1800,
        "Removing the environment from the server…",
        vars,
    )?;
    Ok(Lifecycle {
        up: NonEmpty::new(
            vec![lock()?, init()?, apply("apply", "Creating the environment's VMs on the server…")?],
            "lifecycle.up",
        )?,
        down: NonEmpty::new(vec![lock()?, init()?, destroy], "lifecycle.down")?,
        park: None,
        resume: Vec::new(),
        provision: None,
        seal: None,
        status: Step::action(id("state")?, Action::TerraformReadState),
    })
}

/// The apply succeeded (`applied` says how far the machines' set-up then is), then each Linux
/// VM's set-up and the controller's playbooks are done (on ESXi, Terraform's own set-up wrote
/// each ready file during the apply). Each ready file names `log`, the file on the machine whose
/// tail explains a failed step, when the server's set-up writes one.
pub(super) fn ready(env: &Environment, applied: &str, log: Option<&str>) -> Result<Vec<Condition>, ManifestError> {
    let mut out = vec![Condition::StepOk {
        step: id("apply")?,
        means: Some(applied.into()),
    }];
    for Vm { plan: mp, .. } in env.vms().filter(|vm| !vm.plan.is_windows()) {
        if let Some(a) = mp.addresses.values().next() {
            out.push(ready_file(&a.to_string(), log, SETUP_S)?);
        }
    }
    if controller::needed(env)
        && let Some(at) = controller::addresses(env).values().next()
    {
        out.push(ready_file(&at.to_string(), log, PLAYBOOKS_S)?);
    }
    Ok(out)
}

/// The ready file of the machine at `host`, read through the router, with the set-up's `log`.
fn ready_file(host: &str, log: Option<&str>, within: u32) -> Result<Condition, ManifestError> {
    manifest::ready_file(Template::text(host), USER, log, Some(through_router()?), within)
}

/// `isoloom@${outputs.ip}`: the router, the machines' jump host.
fn through_router() -> Result<Template, ManifestError> {
    Ok(Template::from_parts([
        Part::Text(format!("{USER}@")),
        Part::Placeholder(Placeholder::Output(SnakeName::parse("ip")?)),
    ]))
}

/// A check runner per position (`crate::machine::check_machine`): its script piped to its
/// machine's `sh -s` through the router, the derived checks' switch in its environment, for its
/// checks' waits plus a margin.
pub(super) fn runners(env: &Environment) -> Result<Vec<CheckRunner>, ManifestError> {
    manifest::check_runners(env, |pos, _, group| {
        let Some((_, host)) = check_host(env, pos) else { return Ok(None) };
        let derived = Template::from_parts([
            Part::Text(format!("{}=", isoloom_spec::contract::DERIVED)),
            Part::Placeholder(Placeholder::Param(EnvName::parse(isoloom_spec::contract::DERIVED)?)),
            Part::Text(" sh -s".into()),
        ]);
        let login = Login::new(Template::text(&host), USER);
        let mut step = login.step(&format!("check-{}", pos.id()), &derived, &[("via", through_router()?)])?;
        step.stdin = Some(PathTemplate::relative(&RelPath::parse(&format!("checks/{}.sh", pos.id()))?));
        step.timeout_s = secs(timeout_s(group));
        Ok(Some(step))
    })
}
