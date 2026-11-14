//! What a cloud run does: Terraform's `init` then `apply` (and `destroy`), the apply bringing
//! the project into each machine, waiting for its set-up and sealing the networks without
//! internet itself (`crate::ready`, `crate::cloud::vm::seal`); when it counts as ready (the
//! apply, then each machine's ready file, which says so already); and the param saying whether
//! the derived checks run.

use isoloom_manifest::{
    Action, CheckRunner, Condition, EnvName, Error as ManifestError, Lifecycle, NonEmpty, Param, Part, Placeholder, RunVar, Step, Template,
};
use isoloom_render::steps::PROJECT_DIR;

use crate::machine::user_data::SETUP_LOG;
use crate::module::manifest::{self, id, secs, terraform, words};
use crate::ssh::Login;

/// The values the runner gives a cloud module: its key, which the machines let in, and its
/// private half's file, which Terraform signs in with to bring the project in and wait.
const VARS: &[(&str, RunVar)] = &[("TF_VAR_ssh_public_key", RunVar::SshPublicKey), ("TF_VAR_ssh_private_key_file", RunVar::SshKey)];

/// `0`: the checks derived from the spec are skipped.
use isoloom_spec::contract::DERIVED;

/// A machine the runner delivers the project to and waits for: its name in the manifest, where
/// SSH reaches it, who logs in, how long its set-up may take.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Guest {
    /// Its name in the manifest.
    pub(crate) machine: String,
    /// Its address (`${outputs.address_<machine>}`) and the user the runner's key logs in as.
    pub(crate) login: Login,
    /// How long its set-up may take after the delivery, in seconds.
    pub(crate) setup_s: u32,
}

pub(crate) use manifest::clear_lock;

/// `init`, naming the runner's state.
pub(crate) fn init_step() -> Result<Step, ManifestError> {
    manifest::init(VARS)
}

/// `apply`, under step id `step`, saying `note`.
pub(crate) fn apply_step(step: &str, note: &str) -> Result<Step, ManifestError> {
    manifest::apply(step, note, VARS)
}

/// `destroy`.
pub(crate) fn destroy_step() -> Result<Step, ManifestError> {
    terraform(
        "destroy",
        words(&["destroy", "-auto-approve", "-input=false", "-no-color"]),
        3600,
        "Removing the environment from the cloud…",
        VARS,
    )
}

/// The read-only status probe (the Terraform state).
pub(crate) fn status_step() -> Result<Step, ManifestError> {
    Ok(Step::action(id("state")?, Action::TerraformReadState))
}

/// `up` (its steps already built) and `down`, sharing the lock, init and destroy.
pub(crate) fn terraform_lifecycle(up: Vec<Step>) -> Result<Lifecycle, ManifestError> {
    Ok(Lifecycle {
        up: NonEmpty::new(up, "lifecycle.up")?,
        down: NonEmpty::new(vec![clear_lock()?, init_step()?, destroy_step()?], "lifecycle.down")?,
        park: None,
        resume: Vec::new(),
        provision: None,
        seal: None,
        status: status_step()?,
    })
}

/// A stale lock of a crashed run cleared, then `init`, then `apply` (or `destroy`): the apply
/// ends once every machine is set up and the networks without internet are sealed.
pub(crate) fn lifecycle() -> Result<Lifecycle, ManifestError> {
    terraform_lifecycle(vec![
        clear_lock()?,
        init_step()?,
        apply_step("apply", "Creating the environment's machines in the cloud and waiting for their set-up…")?,
    ])
}

/// The apply succeeded, then each machine's set-up is done (its ready file, over SSH).
pub(crate) fn ready(guests: &[Guest]) -> Result<Vec<Condition>, ManifestError> {
    let mut out = vec![Condition::StepOk {
        step: id("apply")?,
        means: Some("the machines exist and booted; each sets itself up from its user data once the project is in".into()),
    }];
    for g in guests {
        out.push(crate::module::manifest::ready_file(
            g.login.host.clone(),
            &g.login.user,
            Some(SETUP_LOG),
            None,
            g.setup_s,
        )?);
    }
    Ok(out)
}

/// A check runner run on `guest`: its script from the project's copy there, the derived checks'
/// switch in its command.
pub(crate) fn check_step(position: &str, guest: &Guest, script: &str, timeout_s: u32) -> Result<Step, ManifestError> {
    let command = Template::from_parts([
        Part::Text(format!("cd {PROJECT_DIR} && {DERIVED}=")),
        Part::Placeholder(Placeholder::Param(EnvName::parse(DERIVED)?)),
        Part::Text(format!(" sh {script}")),
    ]);
    let mut s = guest.login.step(&format!("check-{position}"), &command, &[])?;
    s.timeout_s = secs(timeout_s);
    Ok(s)
}

/// The derived checks' switch, when the environment has check runners.
pub(crate) fn derived(runners: &[CheckRunner]) -> Result<Vec<Param>, ManifestError> {
    let steps = runners.iter().map(|r| r.step.id.clone()).collect();
    Ok(isoloom_render::run_manifest::derived_switch(steps)?.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guest(user: &str) -> Guest {
        Guest {
            machine: "web".into(),
            login: Login::new(Template::text("203.0.113.10"), user),
            setup_s: 2400,
        }
    }

    #[test]
    fn the_one_apply_brings_the_project_in_waits_and_seals() {
        let l = lifecycle().unwrap();
        let ids: Vec<&str> = l.up.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["clear-lock", "init", "apply"]);
        assert!(l.seal.is_none(), "the module seals within the apply");
        let down: Vec<&str> = l.down.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(down, ["clear-lock", "init", "destroy"]);
    }

    #[test]
    fn a_run_is_ready_once_each_machines_ready_file_says_so() {
        let r = ready(&[guest("admin")]).unwrap();
        assert!(matches!(&r[0], Condition::StepOk { step, .. } if step.as_str() == "apply"));
        let Condition::ReadyFile { user, log, timeout_s, .. } = &r[1] else { panic!() };
        assert_eq!(
            (user.to_string().as_str(), log.as_ref().map(ToString::to_string)),
            ("admin", Some(SETUP_LOG.to_owned()))
        );
        assert_eq!(timeout_s.get(), 2400);
    }
}
