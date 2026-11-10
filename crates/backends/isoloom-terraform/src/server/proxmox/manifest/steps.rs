//! What a Proxmox run does: Terraform's `init` then `apply` (and `destroy`), each with the run's
//! own values in its environment; when it counts as ready (the apply, then each Linux machine's
//! ready file, read through the router); and its check runners, each piped to its machine over
//! SSH through the router.

use isoloom_manifest::{
    Action, CheckRunner, Condition, EnvName, Error as ManifestError, Lifecycle, NonEmpty, Part, PathTemplate, Placeholder, RelPath, RunVar, Seal, SnakeName,
    Step, Template,
};
use isoloom_plan::Environment;
use isoloom_render::run_manifest::{action_with, timeout_s};

use super::slots::SEALED;
use crate::machine::{controller, vms};
use crate::module::manifest::{self, PLAYBOOKS_S, SETUP_S, UNREACHABLE_S, id, output, secs, terraform, words};
use crate::server::router::check_host;

use crate::machine::USER;

/// Where cloud-init writes the set-up's output: its tail explains a failed step.
const SETUP_LOG: &str = "/var/log/cloud-init-output.log";

/// The values the runner gives the module: its slot on the server, its key.
const VARS: &[(&str, RunVar)] = &[
    ("TF_VAR_slot", RunVar::Slot),
    ("TF_VAR_ssh_public_key", RunVar::SshPublicKey),
    ("TF_VAR_proxmox_ssh_private_key_file", RunVar::SshKey),
];

/// The seal reload over SSH: once the second apply has put the sealed snippet in place (for
/// later boots), the runner reloads nftables on the already-booted router from its `SEALED_NFT`
/// file, so the seal takes effect live — the host-side snippet change alone never reaches a
/// running guest (`crate::server::router::sealed_reload`). Reached directly at the router's
/// uplink address (`${outputs.ip}`), the lab's way in, as `isoloom`.
fn seal_reload() -> Result<Step, ManifestError> {
    let with = [
        ("host", output("ip")?),
        ("user", Template::text(USER)),
        ("command", Template::text(&crate::server::router::sealed_reload())),
    ];
    let mut step = Step::action(id("seal-reload")?, Action::SshRun);
    step.run = action_with(Action::SshRun, with)?;
    step.timeout_s = secs(UNREACHABLE_S);
    Ok(step)
}

/// A stale lock of a crashed run cleared, then `init`, then `apply` (or `destroy`); when the run
/// has an `internet: false` network to seal (`sealed`), the seal: a second apply with
/// `isoloom_sealed` on (so later boots are sealed), then an `ssh.run` reload applying the sealed
/// ruleset live to the running router, once the machines are ready (`crate::server::router`).
pub(super) fn lifecycle(sealed: bool) -> Result<Lifecycle, ManifestError> {
    let (lock, init) = (manifest::clear_lock, || manifest::init(VARS));
    let apply = |step, note| manifest::apply(step, note, VARS);
    // No refresh: the state names everything to remove, and a router whose guest agent never
    // ran would hold a refresh for the agent's whole wait.
    let destroy = terraform(
        "destroy",
        words(&["destroy", "-auto-approve", "-input=false", "-no-color", "-refresh=false"]),
        1800,
        "Removing the environment from the server…",
        VARS,
    )?;
    let seal = sealed
        .then(|| {
            Ok::<Seal, ManifestError>(Seal {
                param: EnvName::parse(SEALED)?,
                steps: NonEmpty::new(
                    vec![apply("seal", "Closing the internet to the networks without it…")?, seal_reload()?],
                    "lifecycle.seal.steps",
                )?,
            })
        })
        .transpose()?;
    Ok(Lifecycle {
        up: NonEmpty::new(
            vec![lock()?, init()?, apply("apply", "Creating the environment's VMs on the server…")?],
            "lifecycle.up",
        )?,
        down: NonEmpty::new(vec![lock()?, init()?, destroy], "lifecycle.down")?,
        park: None,
        resume: Vec::new(),
        provision: None,
        seal,
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
    manifest::ready_file(Template::text(host), USER, SETUP_LOG, Some(through_router()?), within)
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
            Part::Text(format!("{}=", isoloom_render::run_manifest::DERIVED)),
            Part::Placeholder(Placeholder::Param(EnvName::parse(isoloom_render::run_manifest::DERIVED)?)),
            Part::Text(" sh -s".into()),
        ]);
        let with = [
            ("host", Template::text(&host)),
            ("user", Template::text(USER)),
            ("via", through_router()?),
            ("command", derived),
        ];
        let mut step = Step::action(id(&format!("check-{}", pos.id()))?, Action::SshRun);
        step.run = action_with(Action::SshRun, with)?;
        step.stdin = Some(PathTemplate::relative(&RelPath::parse(&format!("checks/{}.sh", pos.id()))?));
        step.timeout_s = secs(timeout_s(group));
        Ok(Some(step))
    })
}
