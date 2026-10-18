//! The check runners of a Vagrant run: each VM's `checks` provisioner (`vagrant provision <vm>
//! --provision-with checks`), the lab's Linux VMs first, then the controller's, which stands on
//! every network. The controller is booted for its runner first (a setup step: its boot is not
//! check time) and powered off after. Each runner's time limit is the docker target's rule.

use std::num::NonZeroU32;

use isoloom_manifest::{Action, CheckRunner, Name, Step, StepId};
use isoloom_plan::Environment;
use isoloom_render::run_manifest::{act, check, noted, timeout_s};

use super::lifecycle::park_controller;
use super::vagrant_argv;
use crate::error::Result;
use crate::vms::{CONTROLLER, Shape};

/// How long the controller may take to boot (or to finish halting first).
const WAKE_TIMEOUT_S: u32 = 1800;

/// The id of the step running `vm`'s checks.
fn step_id(vm: &str) -> String {
    let safe: String = vm
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    format!("check-{}", safe.trim_start_matches(CONTROLLER_PREFIX))
}

/// What the controller's step id leaves out of its define name (`check-controller`).
const CONTROLLER_PREFIX: &str = "isoloom-";

/// The runners, in the order the Vagrantfile has their provisioners.
pub(super) fn runners(shape: &Shape) -> Result<Vec<CheckRunner>> {
    shape
        .runners
        .iter()
        .map(|r| {
            let group: Vec<&_> = r.checks.iter().collect();
            let mut step = Step::argv(
                StepId::parse(&step_id(&r.vm))?,
                vagrant_argv(&["provision", &r.vm, "--provision-with", "checks"])?,
            );
            step.timeout_s = NonZeroU32::new(timeout_s(&group));
            Ok(CheckRunner {
                id: Name::parse(&r.vm)?,
                from: r.position.label(),
                checks: group.iter().map(|c| check(c)).collect(),
                step,
                on_timeout: Vec::new(),
            })
        })
        .collect()
}

/// Whether the controller runs checks.
fn controller_checks(shape: &Shape) -> bool {
    shape.runners.iter().any(|r| r.vm == CONTROLLER)
}

/// Before the runners: the controller booted, when it runs checks (waiting for one still
/// halting first: `vagrant.wake_controller`).
pub(super) fn setup(env: &Environment, shape: &Shape) -> Result<Vec<Step>> {
    if !controller_checks(shape) || super::keeps_controller(env) {
        return Ok(Vec::new());
    }
    let mut wake = act("wake-controller", Action::VagrantWakeController)?;
    wake.timeout_s = NonZeroU32::new(WAKE_TIMEOUT_S);
    Ok(vec![noted(wake, "Booting the setup machine for the checks…")])
}

/// After the runners, whatever happened: the controller powered off again.
pub(super) fn teardown(env: &Environment, shape: &Shape) -> Result<Vec<Step>> {
    if !controller_checks(shape) {
        return Ok(Vec::new());
    }
    Ok(park_controller(env, shape, true)?.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runner_step_is_named_after_its_vm() {
        assert_eq!(step_id("web"), "check-web");
        assert_eq!(step_id("isoloom-controller"), "check-controller");
        assert_eq!(step_id("Web_01"), "check-web-01");
    }
}
