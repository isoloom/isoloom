//! Compose inside a VM (`docker-vm`, `cloud-docker`): the lab's containers, and the shell commands
//! that start, check and read them there, as the Compose backend works them out
//! ([`GuestCompose`]). The backend that makes the VM runs each command through its own way in
//! (`vagrant ssh`, or SSH to a VM Terraform made) and adds the VM itself. Shared here, below the
//! backends, so none depends on another and Compose in a VM exists once.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use isoloom_manifest::{
    Action, Check, CheckProtocol, CheckRunner, Checks, Condition, Delivery, Error as ManifestError, Failure, Guest, Machine, Name, Network, Param, PublishMode,
    Resources, Role, Run, SecretSlot, Step, StepId, Template,
};
use isoloom_plan::Environment;

use crate::container::{DOCKER_HOST, HostSize};

/// Isoloom's outputs a Compose VM reads in its copy of the project: the `docker` target's folder
/// (its Compose file, check scripts and builds) and the configuration files its images copy.
/// Never another target's folder, nor what the tools write beside them.
pub const COMPOSE_OUTPUTS: [&str; 2] = [".isoloom/docker", crate::configure::DIR];

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
    /// What its failure does: the run stops, or it is only logged (a clean-up).
    pub failure: Failure,
}

impl GuestCommand {
    /// The step running the command through `way_in` (the VM's: a `vagrant ssh` argv, an
    /// `ssh.run`), with its id, note, time limit and failure policy.
    ///
    /// # Errors
    ///
    /// What `way_in` refuses.
    pub fn step(&self, way_in: &dyn Fn(&Template) -> Result<Run, ManifestError>) -> Result<Step, ManifestError> {
        let mut s = Step::action(self.id.clone(), Action::SshRun);
        s.run = way_in(&self.command)?;
        s.note.clone_from(&self.note);
        s.timeout_s = NonZeroU32::new(self.timeout_s);
        s.failure = self.failure;
        Ok(s)
    }

    /// `commands` as steps through `way_in`, in order.
    ///
    /// # Errors
    ///
    /// What `way_in` refuses.
    pub fn steps(commands: &[Self], way_in: &dyn Fn(&Template) -> Result<Run, ManifestError>) -> Result<Vec<Step>, ManifestError> {
        commands.iter().map(|c| c.step(way_in)).collect()
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
    /// After the check runners, whatever happened: the stand-in taken down, so the access
    /// machine it stands in for can take its addresses.
    pub check_teardown: Vec<GuestCommand>,
    /// The check runners.
    pub runners: Vec<GuestRunner>,
    /// Reads the containers: `docker compose ps --all --format json`, nothing before Docker is in.
    pub status: GuestCommand,
}

impl GuestCompose {
    /// The checks run in the VM through `way_in`: the stand-ins set up, the runners, then the
    /// stand-ins taken down.
    ///
    /// # Errors
    ///
    /// What `way_in` refuses.
    pub fn checks(&self, way_in: &dyn Fn(&Template) -> Result<Run, ManifestError>) -> Result<Checks, ManifestError> {
        Ok(Checks {
            protocol: CheckProtocol::V1,
            setup: GuestCommand::steps(&self.check_setup, way_in)?,
            runners: self.runners.iter().map(|r| r.runner(way_in)).collect::<Result<_, _>>()?,
            teardown: GuestCommand::steps(&self.check_teardown, way_in)?,
        })
    }

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

/// The VM the containers run in, as a run manifest has it, the same on every target that makes
/// one ([`GuestCompose::docker_host`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerHost {
    /// The VM: Isoloom's own ([`DOCKER_HOST`], `infra`), sized for every container at once.
    pub machine: Machine,
    /// When the lab counts as ready: the step that made the VM succeeded, then the containers'.
    pub ready: Vec<Condition>,
    /// Where the runner reads the containers: in the VM, at its own address when it has one.
    pub guest: Guest,
}

impl GuestCompose {
    /// The VM the containers of `env` run in: ready once `made` (a step id, what its success
    /// means) succeeded and the containers are; read through `way_in`, at `address` when the VM
    /// has an address of its own (`None`: this computer's, through Vagrant).
    ///
    /// # Errors
    ///
    /// A name or step id the manifest refuses (a bug: they are fixed), or what `way_in` refuses.
    pub fn docker_host(
        &self,
        env: &Environment,
        made: (&str, &str),
        address: Option<Template>,
        way_in: &dyn Fn(&Template) -> Result<Run, ManifestError>,
    ) -> Result<DockerHost, ManifestError> {
        let size = HostSize::of(env);
        let mut machine = super::machine(DOCKER_HOST, Role::Infra)?;
        machine.resources = Some(Resources {
            cpus: Some(size.cpus),
            memory_mb: Some(size.memory_mb),
        });
        let (step, means) = made;
        let mut ready = vec![Condition::StepOk {
            step: StepId::parse(step)?,
            means: Some(means.to_owned()),
        }];
        ready.extend(self.ready.iter().cloned());
        let guest = Guest {
            machine: Name::parse(DOCKER_HOST)?,
            host: address,
            status: self.status.step(way_in)?,
        };
        Ok(DockerHost { machine, ready, guest })
    }
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
            failure: Failure::Fail,
        }
    }

    fn vagrant_ssh(command: &Template) -> Result<Run, ManifestError> {
        let args = vec![Template::text("ssh"), Template::text("-c"), command.clone()];
        Ok(Run::Argv(HostArgv::Every(Argv::new(Program::parse("vagrant")?, args))))
    }

    /// A sensitive input slot delivered to every step as the environment variable `var`.
    fn slot(name: &str, var: &str) -> SecretSlot {
        SecretSlot {
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
        }
    }

    /// A guest compose carrying only `slots`: everything else empty, with a plain `ps` status.
    fn guest(slots: Vec<SecretSlot>) -> GuestCompose {
        GuestCompose {
            slots,
            ..crate::test_support::empty_guest(command("ps", "docker compose ps"))
        }
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
    fn the_variables_forwarded_are_the_slots_each_once() {
        let g = guest(vec![slot("TOKEN", "TOKEN"), slot("DEMO_SECRET", "DEMO_SECRET"), slot("again", "TOKEN")]);
        assert_eq!(g.forwarded(), ["DEMO_SECRET", "TOKEN"]);
    }
}
