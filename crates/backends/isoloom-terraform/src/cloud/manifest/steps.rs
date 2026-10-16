//! What a cloud run does: Terraform's `init` then `apply` (and `destroy`); the project archived
//! and piped into each machine over SSH, retried while the machine boots; when it counts as ready
//! (the apply, then each machine's ready file); the seal (a second apply with `isoloom_sealed`);
//! and the param saying whether the derived checks run.

use std::collections::BTreeMap;

use isoloom_manifest::{
    Action, Argv, CheckRunner, Condition, EnvName, Error as ManifestError, HostArgv, Lifecycle, NonEmpty, Param, Part, PathTemplate, Placeholder, Program,
    ReadyGrammar, RelPath, Run, RunVar, Seal, SnakeName, Step, Template,
};
use isoloom_render::run_manifest::{noted, run};
use isoloom_render::steps::PROJECT_DIR;

use super::connection::SEALED;
use crate::machine::user_data::{DELIVERED, SETUP_LOG};
use crate::module::manifest::{id, output, run_var, secs, terraform_env, words};

/// How long the runner retries a machine still booting before the delivery gives up, in
/// seconds: within the time its set-up waits for the project.
const BOOT_WAIT_S: u32 = 900;

/// How long a machine may stay out of reach over SSH, once it answered, before the run fails.
const UNREACHABLE_S: u32 = 600;

/// `0`: the checks derived from the spec are skipped.
const DERIVED: &str = "ISOLOOM_DERIVED";

/// A machine the runner delivers the project to and waits for: its name in the manifest, where
/// SSH reaches it, who logs in, how long its set-up may take.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Guest {
    /// Its name in the manifest.
    pub(crate) machine: String,
    /// Its address (`${outputs.address_<machine>}`).
    pub(crate) address: Template,
    /// The user the runner's key logs in as.
    pub(crate) user: String,
    /// How long its set-up may take after the delivery, in seconds.
    pub(crate) setup_s: u32,
}

impl Guest {
    /// `sudo ` unless the user is root (Linode's Debian has no `sudo`).
    fn sudo(&self) -> &'static str {
        if self.user == "root" { "" } else { "sudo " }
    }

    /// Its step id's suffix: its name, as a step id takes it.
    fn id(&self) -> String {
        self.machine.to_ascii_lowercase().replace(['_', '.'], "-")
    }
}

/// A `terraform` step running `args` (`init`'s last one names the runner's state): its data
/// folder in the run's state folder, no prompt, and the key the machines let in.
fn terraform(step: &str, args: Vec<Template>, timeout: u32, note: &str) -> Result<Step, ManifestError> {
    let command = HostArgv::Every(Argv::new(Program::parse("terraform")?, args));
    let mut s = noted(run(step, command, timeout)?, note);
    s.env = terraform_env(vec![("TF_VAR_ssh_public_key", Template::from(Placeholder::Run(RunVar::SshPublicKey)))])?;
    Ok(s)
}

/// An `ssh.run` step to `guest`, running `command`, its `with` extended by `more`.
fn ssh_run(step: &str, guest: &Guest, command: Template, more: &[(&str, Template)]) -> Result<Step, ManifestError> {
    let mut with = vec![("host", guest.address.clone()), ("user", Template::text(&guest.user)), ("command", command)];
    with.extend(more.iter().cloned());
    let mut s = Step::action(id(step)?, Action::SshRun);
    s.run = Run::Action {
        action: Action::SshRun,
        with: with
            .into_iter()
            .map(|(k, v)| Ok((SnakeName::parse(k)?, v)))
            .collect::<Result<_, ManifestError>>()?,
    };
    Ok(s)
}

/// The project archived in the run's state folder (from the project's folder, three levels up
/// from the module's).
fn archive() -> Result<Step, ManifestError> {
    let mut s = Step::action(id("archive")?, Action::ProjectArchive);
    s.run = Run::Action {
        action: Action::ProjectArchive,
        with: BTreeMap::from([(SnakeName::parse("root")?, Template::text("../../.."))]),
    };
    Ok(noted(s, "Archiving the project…"))
}

/// The archive piped into `guest`'s `/opt/isoloom`, then the mark its set-up waits for. Retried
/// while the machine boots (ssh can't connect yet); unpacking again is harmless.
fn deliver(guest: &Guest) -> Result<Step, ManifestError> {
    let command = format!(
        "{}sh -c 'mkdir -p {PROJECT_DIR} /var/lib/isoloom && tar -xzf - -C {PROJECT_DIR} && touch {DELIVERED}'",
        guest.sudo()
    );
    let wait = [("wait_s", Template::text(&BOOT_WAIT_S.to_string()))];
    let mut s = ssh_run(&format!("deliver-{}", guest.id()), guest, Template::text(&command), &wait)?;
    s.stdin = Some(PathTemplate::in_state_dir(&RelPath::parse("project.tgz")?));
    s.timeout_s = secs(BOOT_WAIT_S + 600);
    Ok(noted(s, &format!("Copying the project into {}…", guest.machine)))
}

/// A stale lock of a crashed run cleared, then `init`, then `apply` and the project delivered to
/// each machine (or `destroy`); when the run has anything to seal (`sealed`), the seal: a second
/// apply once the machines are set up.
/// A stale Terraform state lock cleared.
pub(crate) fn clear_lock() -> Result<Step, ManifestError> {
    Ok(Step::action(id("clear-lock")?, Action::TerraformClearLock))
}

/// `init`, naming the runner's state.
pub(crate) fn init_step() -> Result<Step, ManifestError> {
    let mut args = words(&["init", "-input=false", "-no-color"]);
    args.push(run_var("-backend-config=path=", RunVar::StateDir, "/terraform.tfstate"));
    terraform("init", args, 900, "Preparing Terraform and its providers…")
}

/// `apply`, under step id `step`, saying `note`.
pub(crate) fn apply_step(step: &str, note: &str) -> Result<Step, ManifestError> {
    terraform(step, words(&["apply", "-auto-approve", "-input=false", "-no-color"]), 3600, note)
}

/// `destroy`.
pub(crate) fn destroy_step() -> Result<Step, ManifestError> {
    terraform(
        "destroy",
        words(&["destroy", "-auto-approve", "-input=false", "-no-color"]),
        3600,
        "Removing the lab from the cloud…",
    )
}

/// The read-only status probe (the Terraform state).
pub(crate) fn status_step() -> Result<Step, ManifestError> {
    Ok(Step::action(id("state")?, Action::TerraformReadState))
}

/// `up` (its steps already built) and `down`, sharing the lock, init and destroy; `seal` runs
/// after a successful `up`.
fn lifecycle_of(up: Vec<Step>, seal: Option<Seal>) -> Result<Lifecycle, ManifestError> {
    Ok(Lifecycle {
        up: NonEmpty::new(up, "lifecycle.up")?,
        down: NonEmpty::new(vec![clear_lock()?, init_step()?, destroy_step()?], "lifecycle.down")?,
        park: None,
        resume: Vec::new(),
        provision: None,
        seal,
        status: status_step()?,
    })
}

/// `up` (its steps already built) and `down`, sharing the lock, init and destroy.
pub(crate) fn terraform_lifecycle(up: Vec<Step>) -> Result<Lifecycle, ManifestError> {
    lifecycle_of(up, None)
}

/// A stale lock of a crashed run cleared, then `init`, then `apply` and the project delivered to
/// each machine (or `destroy`); when the run has anything to seal (`sealed`), the seal: a second
/// apply once the machines are set up.
pub(crate) fn lifecycle(guests: &[Guest], sealed: bool) -> Result<Lifecycle, ManifestError> {
    let mut up = vec![clear_lock()?, init_step()?, apply_step("apply", "Creating the lab's machines in the cloud…")?];
    if !guests.is_empty() {
        up.push(archive()?);
        up.extend(guests.iter().map(deliver).collect::<Result<Vec<_>, _>>()?);
    }
    let seal = if sealed {
        Some(Seal {
            param: EnvName::parse(SEALED)?,
            steps: NonEmpty::one(apply_step("seal", "Closing the internet to the networks without it…")?),
        })
    } else {
        None
    };
    lifecycle_of(up, seal)
}

/// The apply succeeded, then each machine's set-up is done (its ready file, over SSH).
pub(crate) fn ready(guests: &[Guest]) -> Result<Vec<Condition>, ManifestError> {
    let mut out = vec![Condition::StepOk {
        step: id("apply")?,
        means: Some("the machines exist and booted; each sets itself up from its user data once the project is in".into()),
    }];
    let unset = || ManifestError::OutOfRange {
        what: "a timeout",
        value: 0,
        min: 1,
        max: u32::MAX.into(),
    };
    for g in guests {
        out.push(Condition::ReadyFile {
            host: g.address.clone(),
            user: Template::text(&g.user),
            path: output("ready_file")?,
            log: Some(Template::text(SETUP_LOG)),
            grammar: ReadyGrammar::V1,
            via: None,
            timeout_s: secs(g.setup_s).ok_or_else(unset)?,
            unreachable_timeout_s: secs(UNREACHABLE_S),
            interval_s: secs(10).ok_or_else(unset)?,
        });
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
    let mut s = ssh_run(&format!("check-{position}"), guest, command, &[])?;
    s.timeout_s = secs(timeout_s);
    Ok(s)
}

/// The derived checks' switch, when the lab has check runners.
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
            address: Template::text("203.0.113.10"),
            user: user.into(),
            setup_s: 2400,
        }
    }

    #[test]
    fn the_project_is_archived_once_then_piped_into_each_machine_while_it_boots() {
        let l = lifecycle(
            &[
                guest("admin"),
                Guest {
                    machine: "db".into(),
                    ..guest("root")
                },
            ],
            false,
        )
        .unwrap();
        let ids: Vec<&str> = l.up.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["clear-lock", "init", "apply", "archive", "deliver-web", "deliver-db"]);
        let web = &l.up.as_slice()[4];
        assert_eq!(web.stdin.as_ref().map(ToString::to_string).as_deref(), Some("${run.state_dir}/project.tgz"));
        let Run::Action { with, .. } = &web.run else { panic!() };
        let get = |k: &str| with.iter().find(|(n, _)| n.as_str() == k).map(|(_, v)| v.to_string());
        assert_eq!(get("wait_s").as_deref(), Some("900"));
        assert!(get("command").is_some_and(|c| c.starts_with("sudo sh -c 'mkdir -p /opt/isoloom") && c.contains("touch /var/lib/isoloom/project-delivered")));
        let Run::Action { with: db, .. } = &l.up.as_slice()[5].run else { panic!() };
        assert!(db.values().any(|v| v.to_string().starts_with("sh -c 'mkdir")), "root needs no sudo");
    }

    #[test]
    fn the_seal_is_a_second_apply_switched_by_its_param() {
        assert!(lifecycle(&[guest("admin")], false).unwrap().seal.is_none(), "nothing to seal");
        let l = lifecycle(&[guest("admin")], true).unwrap();
        let seal = l.seal.unwrap();
        assert_eq!(seal.param.as_str(), SEALED);
        let ids: Vec<&str> = seal.steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["seal"]);
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
