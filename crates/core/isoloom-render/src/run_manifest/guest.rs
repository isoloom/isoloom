//! Compose inside a VM (`docker-vm`, `cloud-docker`): the lab's containers, and the shell commands
//! that start, check and read them there, as the Compose backend works them out
//! ([`GuestCompose`]). The backend that makes the VM runs each command through its own way in
//! (`vagrant ssh`, or [`ssh_run`] to a VM Terraform made) and adds the VM itself. A VM Terraform
//! made is readied the same way on every platform first ([`into_vm`]): [`setup_script`] (Docker)
//! piped over SSH, then the project's archive. Shared here, below the backends, so none depends on
//! another and Compose in a VM exists once.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use isoloom_manifest::{
    Action, Check, CheckRunner, Condition, Delivery, Error as ManifestError, Machine, Name, Network, Param, PathTemplate, PublishMode, RelPath, Run,
    SecretSlot, Step, StepId, Template,
};
use isoloom_plan::Environment;

use crate::steps::{CLOUD_INIT_WAIT, INSTALL_DOCKER, PROJECT_DIR, PUBLISHED_FROM_OUTSIDE};

/// A shell command line run in the VM, as one step of the VM's backend. One line for the VM's
/// login shell, with no single quote: `vagrant ssh -c` wraps it in single quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestCommand {
    /// The step's id.
    pub id: StepId,
    /// What the runner says when it starts.
    pub note: Option<String>,
    /// The command line.
    pub command: Template,
    /// How long it may run.
    pub timeout_s: u32,
}

impl GuestCommand {
    /// The step running the command through `way_in` (the VM's: a `vagrant ssh` argv, an
    /// `ssh.run`), with its id, note and time limit.
    ///
    /// # Errors
    ///
    /// What `way_in` refuses.
    pub fn step(&self, way_in: &dyn Fn(&Template) -> Result<Run, ManifestError>) -> Result<Step, ManifestError> {
        let mut s = Step::action(self.id.clone(), Action::SshRun);
        s.run = way_in(&self.command)?;
        s.note.clone_from(&self.note);
        s.timeout_s = NonZeroU32::new(self.timeout_s);
        Ok(s)
    }
}

/// A check runner run in the VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestRunner {
    /// Its id.
    pub id: Name,
    /// Where it runs, for the report.
    pub from: String,
    /// What it reports, in order.
    pub checks: Vec<Check>,
    /// The command that runs it.
    pub command: GuestCommand,
    /// What clears its one-off container once it ran out of time.
    pub on_timeout: Vec<GuestCommand>,
}

impl GuestRunner {
    /// The runner, its commands through `way_in`.
    ///
    /// # Errors
    ///
    /// What `way_in` refuses.
    pub fn runner(&self, way_in: &dyn Fn(&Template) -> Result<Run, ManifestError>) -> Result<CheckRunner, ManifestError> {
        Ok(CheckRunner {
            id: self.id.clone(),
            from: self.from.clone(),
            checks: self.checks.clone(),
            step: self.command.step(way_in)?,
            on_timeout: self.on_timeout.iter().map(|c| c.step(way_in)).collect::<Result<_, _>>()?,
        })
    }
}

/// The lab's containers in a VM, and the commands that run them there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestCompose {
    /// The containers and Compose's own services, as the `docker` target's manifest has them,
    /// each published port `fixed` at its `publish` number (the VM's backend says where it
    /// answers).
    pub machines: Vec<Machine>,
    /// The lab's networks, named as Compose makes them in the VM.
    pub networks: Vec<Network>,
    /// What the containers read at start (inputs, secrets), each delivered as an environment
    /// variable to the [`start`](Self::start) steps creating the containers that read it: the
    /// VM's way in forwards it ([`GuestCompose::forwarded`]).
    pub slots: Vec<SecretSlot>,
    /// Non-secret values the commands read (whether derived checks run).
    pub params: Vec<Param>,
    /// Everything up and healthy, then each leaf job attached.
    pub start: Vec<GuestCommand>,
    /// When the containers count as ready: each start step succeeded.
    pub ready: Vec<Condition>,
    /// Before the check runners (the access machine's stand-in).
    pub check_setup: Vec<GuestCommand>,
    /// The check runners.
    pub runners: Vec<GuestRunner>,
    /// Reads the containers: `docker compose ps --all --format json`, nothing before Docker is in.
    pub status: GuestCommand,
}

impl GuestCompose {
    /// The variables the start steps read from their environment, which the VM's way in must
    /// forward into it (SSH's `SendEnv`, accepted by the VM's `AcceptEnv`): sorted, each once.
    #[must_use]
    pub fn forwarded(&self) -> Vec<String> {
        let vars: BTreeSet<String> = self
            .slots
            .iter()
            .flat_map(|s| s.deliver.iter())
            .filter_map(|d| match d {
                Delivery::Env { var, .. } => Some(var.to_string()),
                _ => None,
            })
            .collect();
        vars.into_iter().collect()
    }

    /// The machines, each published port answering at `address` (this computer's loopback, the
    /// VM's address).
    #[must_use]
    pub fn machines_at(&self, address: &Template) -> Vec<Machine> {
        let mut machines = self.machines.clone();
        for p in machines.iter_mut().flat_map(|m| m.ports.iter_mut()) {
            if let Some(publish) = &mut p.publish {
                publish.mode = PublishMode::Fixed;
                publish.address = Some(address.clone());
            }
        }
        machines
    }
}

/// The way into a VM Terraform made: `ssh.run` to `host` (`${outputs.ip}`) as `user`, with the
/// runner's key.
pub fn ssh_run(host: Template, user: &str) -> impl Fn(&Template) -> Result<Run, ManifestError> {
    let user = Template::text(user);
    move |command: &Template| super::action_with(Action::SshRun, [("host", host.clone()), ("user", user.clone()), ("command", command.clone())])
}

/// How long the set-up script may take: cloud-init's end, then Docker's install.
const SETUP_S: u32 = 1800;

/// A VM Terraform made, readied for Compose through `way_in`: `setup` (the generated
/// [`setup_script`], beside the manifest) piped into `sudo sh -s`, then the project archived
/// (`project.archive`, `root` its folder from the manifest's) and piped into `tar` in the VM's
/// project folder, replacing what an earlier copy left.
///
/// # Errors
///
/// What `way_in` refuses, or a path or id the manifest refuses (a bug: they are fixed).
pub fn into_vm(way_in: &dyn Fn(&Template) -> Result<Run, ManifestError>, setup: &RelPath, root: &str) -> Result<Vec<Step>, ManifestError> {
    let docker = GuestCommand {
        id: StepId::parse("docker")?,
        note: Some("Installing Docker in the VM…".into()),
        command: Template::text("sudo sh -s"),
        timeout_s: SETUP_S,
    };
    let mut docker = docker.step(way_in)?;
    docker.stdin = Some(PathTemplate::relative(setup));
    let mut archive = Step::action(StepId::parse("archive")?, Action::ProjectArchive);
    archive.run = super::action_with(Action::ProjectArchive, [("root", Template::text(root))])?;
    archive.note = Some("Archiving the project…".into());
    let copy = GuestCommand {
        id: StepId::parse("project")?,
        note: Some("Copying the project into the VM…".into()),
        command: Template::text(&format!(
            "sudo rm -rf {PROJECT_DIR} && sudo mkdir -p {PROJECT_DIR} && sudo tar -xzf - -C {PROJECT_DIR}"
        )),
        timeout_s: 600,
    };
    let mut copy = copy.step(way_in)?;
    copy.stdin = Some(PathTemplate::in_state_dir(&RelPath::parse("project.tgz")?));
    Ok(vec![docker, archive, copy])
}

/// The Docker host's set-up script for `env`, run as root (`sudo sh -s`): once cloud-init is done,
/// the spec's constraints (proxy, mirrors, NTP), then Docker from Docker's own script unless it is
/// there, and published ports answering whatever the containers' routes
/// ([`PUBLISHED_FROM_OUTSIDE`]). Running it again changes nothing.
#[must_use]
pub fn setup_script(env: &Environment) -> String {
    let prelude = crate::steps::linux(env, "", &isoloom_plan::Step::Prelude).unwrap_or_default();
    format!(
        "#!/bin/sh\n{}# The Docker host's set-up, piped into `sudo sh -s` over SSH by its run manifest.\nset -e\n{CLOUD_INIT_WAIT}\n{prelude}{INSTALL_DOCKER}\n{PUBLISHED_FROM_OUTSIDE}",
        crate::header("#")
    )
}

#[cfg(test)]
mod tests {
    use isoloom_manifest::{Argv, EnvName, HostArgv, Identifier, NonEmpty, Program, SlotKind, StepSelection};

    use super::*;

    fn command(id: &str, text: &str) -> GuestCommand {
        GuestCommand {
            id: StepId::parse(id).unwrap(),
            note: Some("Starting…".into()),
            command: Template::text(text),
            timeout_s: 60,
        }
    }

    fn vagrant_ssh(command: &Template) -> Result<Run, ManifestError> {
        let args = vec![Template::text("ssh"), Template::text("-c"), command.clone()];
        Ok(Run::Argv(HostArgv::Every(Argv::new(Program::parse("vagrant")?, args))))
    }

    #[test]
    fn a_command_becomes_a_step_of_the_vms_way_in() {
        let s = command("start", "docker compose up").step(&vagrant_ssh).unwrap();
        assert_eq!(s.id.as_str(), "start");
        assert_eq!(s.note.as_deref(), Some("Starting…"));
        assert_eq!(s.timeout_s.map(NonZeroU32::get), Some(60));
        let Run::Argv(HostArgv::Every(argv)) = &s.run else { panic!("{:?}", s.run) };
        assert_eq!(argv.args().last().map(ToString::to_string).as_deref(), Some("docker compose up"));
    }

    #[test]
    fn a_vm_terraform_made_gets_docker_then_the_project_over_ssh() {
        let way_in = ssh_run(Template::text("10.0.0.9"), "isoloom");
        let steps = into_vm(&way_in, &RelPath::parse("setup.sh").unwrap(), "../../..").unwrap();
        let ids: Vec<&str> = steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["docker", "archive", "project"]);
        assert_eq!(steps[0].stdin.as_ref().map(ToString::to_string).as_deref(), Some("setup.sh"));
        assert_eq!(
            steps[2].stdin.as_ref().map(ToString::to_string).as_deref(),
            Some("${run.state_dir}/project.tgz")
        );
        let Run::Action { action: Action::SshRun, with } = &steps[2].run else {
            panic!("{:?}", steps[2].run)
        };
        let pairs: Vec<(String, String)> = with.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        assert_eq!(
            pairs,
            [
                (
                    "command".into(),
                    "sudo rm -rf /opt/isoloom && sudo mkdir -p /opt/isoloom && sudo tar -xzf - -C /opt/isoloom".into()
                ),
                ("host".into(), "10.0.0.9".into()),
                ("user".into(), "isoloom".into())
            ]
        );
        let Run::Action {
            action: Action::ProjectArchive,
            with: root,
        } = &steps[1].run
        else {
            panic!()
        };
        assert_eq!(root.values().map(ToString::to_string).collect::<Vec<_>>(), ["../../.."]);
    }

    #[test]
    fn the_old_copy_cleared_is_always_the_fixed_project_folder() {
        let way_in = ssh_run(Template::text("10.0.0.9"), "isoloom");
        let steps = into_vm(&way_in, &RelPath::parse("setup.sh").unwrap(), "../../..").unwrap();
        let Run::Action { with, .. } = &steps[2].run else { panic!() };
        let command = with.iter().find(|(k, _)| k.as_str() == "command").map(|(_, v)| v.to_string()).unwrap();
        // A literal path, nothing a shell expands: no variable can turn it into `rm -rf /`.
        assert!(command.starts_with("sudo rm -rf /opt/isoloom && "), "{command}");
        assert!(!command.contains('$') && !command.contains('*') && !command.contains('`'), "{command}");
        assert_eq!(PROJECT_DIR, "/opt/isoloom");
    }

    #[test]
    fn the_setup_script_waits_for_cloud_init_then_installs_docker() {
        let env = crate::testing::env(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\n",
            isoloom_spec::model::Target::Docker,
        );
        let script = setup_script(&env);
        assert!(script.starts_with("#!/bin/sh\n# Generated by isoloom"), "{script}");
        assert!(
            script.ends_with(&format!(
                "set -e\ncloud-init status --wait >/dev/null 2>&1 || true\n{INSTALL_DOCKER}\n{PUBLISHED_FROM_OUTSIDE}"
            )),
            "{script}"
        );
    }

    #[test]
    fn the_variables_forwarded_are_the_slots_each_once() {
        let slot = |name: &str, var: &str| SecretSlot {
            name: Identifier::parse(name).unwrap(),
            kind: SlotKind::Input,
            required: false,
            sensitive: true,
            machines: Vec::new(),
            providers: BTreeSet::new(),
            source: None,
            default: None,
            deliver: NonEmpty::one(Delivery::Env {
                var: EnvName::parse(var).unwrap(),
                steps: StepSelection::Every,
            }),
        };
        let guest = GuestCompose {
            machines: Vec::new(),
            networks: Vec::new(),
            slots: vec![slot("TOKEN", "TOKEN"), slot("FLAG", "FLAG"), slot("again", "TOKEN")],
            params: Vec::new(),
            start: Vec::new(),
            ready: Vec::new(),
            check_setup: Vec::new(),
            runners: Vec::new(),
            status: command("ps", "docker compose ps"),
        };
        assert_eq!(guest.forwarded(), ["FLAG", "TOKEN"]);
    }
}
