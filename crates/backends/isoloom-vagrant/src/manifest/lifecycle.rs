//! The operations of a Vagrant run (run-manifest-v0.md §10), when it counts as ready, and how a
//! start that tripped over a previous run's leftovers is retried. The controller is powered off
//! once the lab is built (it comes back for provisioning and checks), except on QEMU, where it is
//! the player's position on the lab network, and when the spec keeps it running.

use isoloom_manifest::{Action, Condition, Lifecycle, Name, NonEmpty, Park, Placeholder, Provider, Provision, Recovery, Retries, Step, StepId, Template};
use isoloom_plan::Environment;
use isoloom_render::run_manifest::{act, cleanup, noted, run, tolerated};

use super::{keeps_controller, on_provider, vagrant_argv, vagrant_with};
use crate::error::Result;
use crate::vms::{CONTROLLER, Kind, Shape};

/// The start step: the step the values given at launch reach, with the provision steps.
pub(super) const UP: &str = "up";
/// Every machine but the controller set up again.
pub(super) const PROVISION: &str = "provision";
/// One machine set up again.
pub(super) const PROVISION_MACHINE: &str = "provision-machine";

/// How long `vagrant up` may run: a lab of Windows VMs emulated by QEMU can take hours, and the
/// runner keeps no shorter limit than the tool would.
const UP_TIMEOUT_S: u32 = 21_600;
/// Halting or suspending every VM.
const PARK_TIMEOUT_S: u32 = 1800;
/// Halting the controller alone.
const CONTROLLER_HALT_S: u32 = 300;

/// What a failed start's output says when it tripped over a previous run's leftovers, not over
/// the lab (VirtualBox's names taken, a lock, a record on another provider).
const STALE_MARKERS: [&str; 5] = [
    "verr_already_exists",
    "could not rename",
    "is locked",
    "different provider",
    "single provider at a time",
];

/// Ansible's failure line on the controller: the last one leads a failed step's error.
const ANSIBLE_VERDICT: &str = "fatal: \\[[^\\]]+\\]: (UNREACHABLE|FAILED)!.*";

/// Up, down, park, resume, provision and status, as Vagrant commands and the driver's actions.
pub(super) fn lifecycle(env: &Environment, shape: &Shape) -> Result<Lifecycle> {
    let virtualbox = || -> Result<Step> {
        let mut s = tolerated(act("discard-saved", Action::VagrantDiscardAbortedSaved)?);
        s.providers.insert(Provider::Virtualbox);
        Ok(s)
    };
    let mut up = vec![
        noted(
            act("reconcile", Action::VagrantReconcileProvider)?,
            "Clearing the state of a run on another provider, if any…",
        ),
        virtualbox()?,
        up_step(shape)?,
    ];
    up.extend(park_controller(env, shape, false)?);
    let resume = resume(env, shape, virtualbox)?;
    let park = |verb: &str| -> Result<Option<Vec<Step>>> { Ok(Some(vec![run(verb, vagrant_argv(&[verb])?, PARK_TIMEOUT_S)?])) };
    Ok(Lifecycle {
        up: NonEmpty::new(up, "lifecycle.up")?,
        down: NonEmpty::new(down()?, "lifecycle.down")?,
        park: Some(Park {
            pause: park("suspend")?,
            shutdown: park("halt")?,
        }),
        resume,
        provision: Some(provision(env, shape)?),
        status: run("status", vagrant_argv(&["status", "--machine-readable"])?, 25)?,
    })
}

/// `vagrant up --provider ${run.provider}`, whose success means every VM booted and ran its
/// provisioners, the controller's playbooks included.
fn up_step(shape: &Shape) -> Result<Step> {
    let mut s = run(UP, on_provider(&["up"], &[])?, UP_TIMEOUT_S)?;
    s.note = Some("Building the lab's VMs (Windows machines take a while)…".into());
    if shape.controller().is_some() {
        s.verdict = Some(ANSIBLE_VERDICT.into());
    }
    Ok(s)
}

/// The parked VMs back: a suspended one resumes, a halted one boots, the controller stays off.
/// A saved state the hypervisor can't restore (VirtualBox 7.2 refuses its own VMSVGA saved
/// states: `Failed to load unit 'vga'`) leaves the VM `aborted-saved`: its cut-off state is
/// discarded and the VM boots from its disk instead, so the lab comes back without its memory
/// rather than not at all. A second `up` of running VMs is a no-op.
fn resume(env: &Environment, shape: &Shape, virtualbox: impl Fn() -> Result<Step>) -> Result<Vec<Step>> {
    let up = |id: &str| -> Result<Step> { Ok(run(id, on_provider(&["up", "--no-provision"], &running(env, shape))?, 3600)?) };
    let mut cut_off = virtualbox()?;
    cut_off.id = StepId::parse("discard-cut-off")?;
    Ok(vec![virtualbox()?, tolerated(up("resume")?), cut_off, up("boot")?])
}

/// Every VM that runs while the lab does, which a resume brings back (`vagrant up
/// --no-provision`: a suspended one resumes, a halted one boots): the controller stays off.
fn running<'a>(env: &Environment, shape: &'a Shape) -> Vec<&'a str> {
    let keep = keeps_controller(env);
    shape
        .defines
        .iter()
        .filter(|d| d.kind != Kind::Controller || keep)
        .map(|d| d.name.as_str())
        .collect()
}

/// The controller halted: after `up` (tolerated: it powers itself off after its play), after a
/// re-provision and after the checks (`always`). Not on QEMU, nor when the spec keeps it on.
pub(super) fn park_controller(env: &Environment, shape: &Shape, always: bool) -> Result<Option<Step>> {
    if shape.controller().is_none() || keeps_controller(env) {
        return Ok(None);
    }
    let halt = run("park-controller", vagrant_argv(&["halt", CONTROLLER])?, CONTROLLER_HALT_S)?;
    let mut s = if always { cleanup(halt) } else { tolerated(halt) };
    s.note = Some("Powering off the setup machine (it comes back for provisioning and checks)…".into());
    s.providers = [
        Provider::Virtualbox,
        Provider::VmwareDesktop,
        Provider::Hyperv,
        Provider::Parallels,
        Provider::Libvirt,
        Provider::Utm,
        Provider::VmwareEsxi,
    ]
    .into();
    Ok(Some(s))
}

/// Destroy every VM, then this run's VMs Vagrant lost track of, then Vagrant's state (only once
/// the VMs are gone: a failed destroy keeps it for the next try).
fn down() -> Result<Vec<Step>> {
    let mut destroy = run("destroy", vagrant_argv(&["destroy", "--force"])?, 1800)?;
    destroy.when = isoloom_manifest::When::Always;
    Ok(vec![destroy, orphans()?, act("reset", Action::VagrantResetState)?])
}

/// `vagrant.remove_orphans`, on the providers it knows (VirtualBox and Parallels).
fn orphans() -> Result<Step> {
    let mut s = cleanup(act("orphans", Action::VagrantRemoveOrphans)?);
    s.providers = [Provider::Virtualbox, Provider::Parallels].into();
    Ok(s)
}

/// Set-up again: every machine but the controller (`vagrant provision <vm>…`), or one
/// (`${machine}`); then, when the spec has playbooks, the controller's play.
fn provision(env: &Environment, shape: &Shape) -> Result<Provision> {
    let vms: Vec<&str> = shape.defines.iter().filter(|d| d.kind != Kind::Controller).map(|d| d.name.as_str()).collect();
    let mut all_args = vec!["provision"];
    all_args.extend(&vms);
    let all = run(PROVISION, vagrant_argv(&all_args)?, 7200)?;
    let machine = vagrant_with(vec![Template::text("provision"), Template::from(Placeholder::Machine)])?;
    let one = run(PROVISION_MACHINE, machine, 7200)?;
    if env.spec().provision.is_empty() || shape.controller().is_none() {
        return Ok(Provision {
            all: vec![all],
            machine: vec![one],
        });
    }
    let wake = || -> Result<Step> {
        let s = run("wake-controller", vagrant_argv(&["up", CONTROLLER, "--no-provision"])?, 1800)?;
        Ok(noted(s, "Booting the setup machine…"))
    };
    let play = || -> Result<Step> {
        let mut s = run("play", vagrant_argv(&["provision", CONTROLLER, "--provision-with", "ansible"])?, 14_400)?;
        s.verdict = Some(ANSIBLE_VERDICT.into());
        Ok(noted(s, "Running the lab's setup play…"))
    };
    let park = || park_controller(env, shape, true);
    Ok(Provision {
        all: [Some(wake()?), Some(all), Some(play()?), park()?].into_iter().flatten().collect(),
        machine: [Some(wake()?), Some(one), Some(play()?), park()?].into_iter().flatten().collect(),
    })
}

/// `up` succeeded (every VM booted and ran its provisioners), then every lab machine runs.
pub(super) fn ready(shape: &Shape) -> Result<Vec<Condition>> {
    let up = Condition::StepOk {
        step: StepId::parse(UP)?,
        means: Some("every VM booted and ran its provisioners, the controller's playbooks included".into()),
    };
    let lab: Vec<Name> = shape
        .defines
        .iter()
        .filter(|d| d.kind == Kind::Lab)
        .map(|d| Name::parse(&d.name))
        .collect::<Result<_, _>>()?;
    Ok(vec![up, Condition::MachinesRunning { machines: lab }])
}

/// A start whose output says it tripped over leftovers (`STALE_MARKERS`) clears them (destroy,
/// orphans, Vagrant's state) and tries once more. A genuine failure is never retried.
pub(super) fn recovery() -> Result<Recovery> {
    let mut destroy = cleanup(run("destroy", vagrant_argv(&["destroy", "--force"])?, 180)?);
    destroy.note = Some("Clearing what an earlier run left, then starting again…".into());
    let mut reset = act("reset", Action::VagrantResetState)?;
    reset.when = isoloom_manifest::When::Always;
    Ok(Recovery {
        stale_markers: STALE_MARKERS.map(String::from).into(),
        steps: vec![destroy, orphans()?, reset],
        retries: Retries::new(1)?,
        apply_retries: None,
    })
}
