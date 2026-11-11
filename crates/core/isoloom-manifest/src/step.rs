//! Steps: one native command (an argv) or one documented driver action, with the folder,
//! environment, input, time limit and failure policy it runs under.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::argv::HostArgv;
use crate::identity::{HostOs, Provider};
use crate::names::{EnvName, SnakeName, StepId};
use crate::path::{PathTemplate, RelPath};
use crate::template::Template;
use crate::{Error, Result};

/// A driver primitive: a read-then-act sequence one argv can't express (list, then remove what
/// was listed). The manifest's documentation fixes each action's native commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum Action {
    /// Pins the ephemeral host ports of a fresh run in the pinned-ports file.
    #[serde(rename = "compose.pin_ports")]
    ComposePinPorts,
    /// Fails with `PortInUse` when a published host port is taken (unless the run is up).
    #[serde(rename = "compose.check_ports_free")]
    ComposeCheckPortsFree,
    /// Removes the project's networks its Compose files no longer define.
    #[serde(rename = "compose.remove_stale_networks")]
    ComposeRemoveStaleNetworks,
    /// Removes the project's containers (`with`: `service`, `one_off`).
    #[serde(rename = "compose.remove_containers")]
    ComposeRemoveContainers,
    /// Unplugs and removes every network of the project.
    #[serde(rename = "compose.remove_networks")]
    ComposeRemoveNetworks,
    /// Destroys a run recorded under another provider.
    #[serde(rename = "vagrant.reconcile_provider")]
    VagrantReconcileProvider,
    /// Discards a VirtualBox VM's aborted saved state.
    #[serde(rename = "vagrant.discard_aborted_saved")]
    VagrantDiscardAbortedSaved,
    /// Removes this run's VMs Vagrant lost track of, by their exact names.
    #[serde(rename = "vagrant.remove_orphans")]
    VagrantRemoveOrphans,
    /// Removes Vagrant's state folder.
    #[serde(rename = "vagrant.reset_state")]
    VagrantResetState,
    /// Boots the controller, waiting for a halting one first.
    #[serde(rename = "vagrant.wake_controller")]
    VagrantWakeController,
    /// Halts the controller, waiting for one that powers itself off.
    #[serde(rename = "vagrant.park_controller")]
    VagrantParkController,
    /// Fails with `PortInUse` when a host port the VMs forward (a published port with a `fixed`
    /// host port) is taken, unless the run's VMs exist: two runs forwarding one port collide.
    #[serde(rename = "vagrant.check_forwarded_ports")]
    VagrantCheckForwardedPorts,
    /// Removes a stale Terraform state lock.
    #[serde(rename = "terraform.clear_lock")]
    TerraformClearLock,
    /// Reads the Terraform state's outputs, resources and taint.
    #[serde(rename = "terraform.read_state")]
    TerraformReadState,
    /// Copies the environment's own Terraform module (`with`: `from`, its folder relative to the
    /// manifest's, inside the project) into `${run.state_dir}/module`, where the steps run it:
    /// the lock file and what the module writes stay with the run, out of the project. The
    /// copy's Terraform files are replaced (a file renamed or removed in the module isn't applied
    /// any more), what the module wrote there before is kept, and `.terraform` and state files
    /// are left out. `with.when: missing` copies only when there is no copy (a `down` destroys
    /// with the module as it was applied).
    #[serde(rename = "terraform.copy_module")]
    TerraformCopyModule,
    /// Runs a command over SSH with the runner's key (`with`: `host`, `user`, `via`, `command`;
    /// `wait_s`, at most 1800, the seconds the runner retries while ssh itself can't connect
    /// (its exit 255 before the command ran: a machine still booting), its failure then naming the
    /// machine's address. Only for an idempotent command, such as a delivery).
    #[serde(rename = "ssh.run")]
    SshRun,
    /// Archives the project as `${run.state_dir}/project.tgz` (`with`: `root`, the project's folder
    /// relative to the manifest's; `outputs`, optional, the paths of Isoloom's outputs the guest
    /// reads, relative to the project and separated by spaces). What
    /// [`Rules::for_copy`](crate::ignore::Rules::for_copy) leaves out stays out: what
    /// `.isoloomignore` lists, Isoloom's outputs but `outputs`, and, at any depth, version control
    /// and the tools' state (a Terraform state, a Vagrant machine folder, the runner's logins). A
    /// step pipes it to a guest.
    #[serde(rename = "project.archive")]
    ProjectArchive,
}

/// When a step runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum When {
    /// Skipped once a step of the operation failed.
    #[default]
    IfOk,
    /// A clean-up that runs anyway.
    Always,
}

/// What a step's failure does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    /// Fails the operation.
    #[default]
    Fail,
    /// Logs and goes on.
    Tolerate,
}

/// What a step runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Run {
    /// A native command.
    Argv(HostArgv),
    /// A driver action, with its arguments.
    Action {
        /// The action.
        action: Action,
        /// Its arguments (templates).
        with: BTreeMap<SnakeName, Template>,
    },
}

/// One step of an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "StepFields", into = "StepFields")]
pub struct Step {
    /// Its id, unique within its list.
    pub id: StepId,
    /// A line the runner emits when the step starts.
    pub note: Option<String>,
    /// What it runs.
    pub run: Run,
    /// Its working folder (default: the manifest's folder).
    pub cwd: Option<RelPath>,
    /// Its non-secret environment, values as templates.
    pub env: BTreeMap<EnvName, Template>,
    /// A file piped into the program: a generated one beside the manifest (a guest script), or one
    /// of the run's state folder (`${run.state_dir}/project.tgz`). A shell runs in the guest, never
    /// on the host.
    pub stdin: Option<PathTemplate>,
    /// How long it may go without progress before the runner stops it. Progress is an output
    /// line, on either stream, that isn't blank and isn't one it printed just before (a stuck
    /// download repeating its line makes none); a step that prints nothing has the limit from its
    /// start. A step that keeps making progress still stops at a ceiling: six hours from its
    /// start, or this limit when it is longer.
    pub timeout_s: Option<NonZeroU32>,
    /// The same limit on these Vagrant providers, in place of `timeout_s` (only there), counted
    /// the same way: a provider whose tool is slower (`vmware_esxi` asks the ESXi host once per
    /// VM).
    pub provider_timeout_s: BTreeMap<Provider, NonZeroU32>,
    /// A download (the images a start pulls): a runner lets it run as long as it makes progress,
    /// with no ceiling, stops it once it shows none for its limit (`timeout_s`), and reports that
    /// and its failure as a download's, so it can tell a slow or stuck connection from a broken
    /// step.
    pub download: bool,
    /// When it runs.
    pub when: When,
    /// What its failure does.
    pub failure: Failure,
    /// Only on these hosts (empty: every host).
    pub hosts: BTreeSet<HostOs>,
    /// Only on these Vagrant providers (empty: every provider).
    pub providers: BTreeSet<Provider>,
    /// A regular expression over the step's lines; the last match leads the step's error.
    pub verdict: Option<String>,
}

impl Step {
    /// A step running `argv`, with every other field at its default.
    #[must_use]
    pub fn argv(id: StepId, argv: HostArgv) -> Step {
        Step::new(id, Run::Argv(argv))
    }

    /// A step running `action` with no argument, every other field at its default.
    #[must_use]
    pub fn action(id: StepId, action: Action) -> Step {
        Step::new(id, Run::Action { action, with: BTreeMap::new() })
    }

    fn new(id: StepId, run: Run) -> Step {
        Step {
            id,
            note: None,
            run,
            cwd: None,
            env: BTreeMap::new(),
            stdin: None,
            timeout_s: None,
            provider_timeout_s: BTreeMap::new(),
            download: false,
            when: When::IfOk,
            failure: Failure::Fail,
            hosts: BTreeSet::new(),
            providers: BTreeSet::new(),
            verdict: None,
        }
    }
}

/// Whether `v` is its type's default: a field left out when it holds it.
pub(crate) fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// A step as written: `argv` or `action` (with its `with`), never both.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Step",
    description = "One step of an operation: an `argv` or an `action`, never both.",
    extend("oneOf" = [{ "required": ["argv"] }, { "required": ["action"] }], "dependentRequired" = { "with": ["action"] })
)]
struct StepFields {
    /// Its id, unique within its list.
    id: StepId,
    /// A line the runner emits when the step starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    /// A native command, run without a shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    argv: Option<HostArgv>,
    /// A driver action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    action: Option<Action>,
    /// An action's arguments (templates).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    with: BTreeMap<SnakeName, Template>,
    /// Its working folder (default: the manifest's folder).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cwd: Option<RelPath>,
    /// Non-secret environment of this step, values as templates (`TF_DATA_DIR`,
    /// `TF_IN_AUTOMATION`). Secrets and params come through their slots.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    env: BTreeMap<EnvName, Template>,
    /// A file piped into the program: a generated one beside the manifest (a check script into
    /// `docker compose exec -T <m> sh -s` or `ssh … sh -s`), or one of the run's state folder (the
    /// project's archive, `${run.state_dir}/project.tgz`). A shell runs in the guest, never on the
    /// host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stdin: Option<PathTemplate>,
    /// How long it may go without progress, in seconds, before the runner stops it. Progress is
    /// an output line, on either stream, that isn't blank and isn't one it printed just before; a
    /// step that prints nothing has the limit from its start. A step that keeps making progress
    /// still stops at a ceiling: six hours from its start, or this limit when it is longer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout_s: Option<NonZeroU32>,
    /// The same limit on these Vagrant providers, in seconds, in place of `timeout_s` there only
    /// (`{"vmware_esxi": 120}`), counted the same way.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    provider_timeout_s: BTreeMap<Provider, NonZeroU32>,
    /// `true`: a download (the images a start pulls). A runner lets it run as long as it makes
    /// progress, with no ceiling, stops it once it shows none for its limit (`timeout_s`), and
    /// reports that stall and its failure as a download's. Default `false`.
    #[serde(default, skip_serializing_if = "is_default")]
    download: bool,
    /// `if_ok` (default): skipped once a step of the operation failed. `always`: a clean-up that
    /// runs anyway.
    #[serde(default, skip_serializing_if = "is_default")]
    when: When,
    /// `fail` (default) fails the operation; `tolerate` logs and goes on.
    #[serde(default, skip_serializing_if = "is_default")]
    failure: Failure,
    /// Only on these hosts.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    hosts: BTreeSet<HostOs>,
    /// Only on these Vagrant providers (`${run.provider}`).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    providers: BTreeSet<Provider>,
    /// A regular expression over the step's lines; the last match leads the step's error
    /// (Ansible's `fatal: [host]: …`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verdict: Option<String>,
}

impl TryFrom<StepFields> for Step {
    type Error = Error;

    fn try_from(f: StepFields) -> Result<Step> {
        let shape = |reason| Error::StepShape { id: f.id.to_string(), reason };
        let run = match (f.argv, f.action) {
            (Some(_), Some(_)) => return Err(shape("it has both `argv` and `action`: keep one")),
            (None, None) => return Err(shape("it has neither `argv` nor `action`: give one")),
            (Some(_), None) if !f.with.is_empty() => return Err(shape("`with` goes with an `action`, not an `argv`")),
            (Some(argv), None) => Run::Argv(argv),
            (None, Some(action)) => Run::Action { action, with: f.with },
        };
        Ok(Step {
            id: f.id,
            note: f.note,
            run,
            cwd: f.cwd,
            env: f.env,
            stdin: f.stdin,
            timeout_s: f.timeout_s,
            provider_timeout_s: f.provider_timeout_s,
            download: f.download,
            when: f.when,
            failure: f.failure,
            hosts: f.hosts,
            providers: f.providers,
            verdict: f.verdict,
        })
    }
}

impl From<Step> for StepFields {
    fn from(s: Step) -> StepFields {
        let (argv, action, with) = match s.run {
            Run::Argv(a) => (Some(a), None, BTreeMap::new()),
            Run::Action { action, with } => (None, Some(action), with),
        };
        StepFields {
            id: s.id,
            note: s.note,
            argv,
            action,
            with,
            cwd: s.cwd,
            env: s.env,
            stdin: s.stdin,
            timeout_s: s.timeout_s,
            provider_timeout_s: s.provider_timeout_s,
            download: s.download,
            when: s.when,
            failure: s.failure,
            hosts: s.hosts,
            providers: s.providers,
            verdict: s.verdict,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_runs_an_argv_or_an_action() {
        let s: Step = serde_json::from_str(r#"{"id": "up", "argv": ["docker", "compose", "up"], "timeout_s": 1800}"#).unwrap();
        assert!(matches!(s.run, Run::Argv(_)));
        assert_eq!(s.timeout_s.map(NonZeroU32::get), Some(1800));
        let a: Step = serde_json::from_str(r#"{"id": "rm", "action": "compose.remove_containers", "with": {"service": "x"}, "failure": "tolerate"}"#).unwrap();
        assert!(matches!(a.run, Run::Action { action: Action::ComposeRemoveContainers, ref with } if with.len() == 1));
        let back = serde_json::to_string(&a).unwrap();
        assert_eq!(
            back,
            r#"{"id":"rm","action":"compose.remove_containers","with":{"service":"x"},"failure":"tolerate"}"#
        );
    }

    #[test]
    fn a_provider_may_have_its_own_time_limit() {
        let text = r#"{"id":"status","argv":["vagrant","status"],"timeout_s":25,"provider_timeout_s":{"vmware_esxi":120}}"#;
        let s: Step = serde_json::from_str(text).unwrap();
        assert_eq!(s.provider_timeout_s.get(&Provider::VmwareEsxi).map(|t| t.get()), Some(120));
        assert_eq!(serde_json::to_string(&s).unwrap(), text);
        // Left out when no provider has one.
        let plain: Step = serde_json::from_str(r#"{"id":"status","argv":["vagrant","status"]}"#).unwrap();
        assert!(plain.provider_timeout_s.is_empty());
        assert!(!serde_json::to_string(&plain).unwrap().contains("provider_timeout_s"));
        assert!(serde_json::from_str::<Step>(r#"{"id":"s","argv":["v"],"provider_timeout_s":{"vmware_esxi":0}}"#).is_err());
        assert!(serde_json::from_str::<Step>(r#"{"id":"s","argv":["v"],"provider_timeout_s":{"esx":60}}"#).is_err());
    }

    #[test]
    fn a_download_says_so_and_a_step_that_isnt_leaves_it_out() {
        let text = r#"{"id":"pull-start","argv":["vagrant","ssh"],"timeout_s":900,"download":true}"#;
        let s: Step = serde_json::from_str(text).unwrap();
        assert!(s.download);
        assert_eq!(serde_json::to_string(&s).unwrap(), text);
        let plain: Step = serde_json::from_str(r#"{"id":"up","argv":["vagrant","up"]}"#).unwrap();
        assert!(!plain.download);
        assert!(!serde_json::to_string(&plain).unwrap().contains("download"));
    }

    #[test]
    fn a_step_with_both_or_neither_is_refused() {
        let both = r#"{"id": "x", "argv": ["docker"], "action": "ssh.run"}"#;
        assert!(serde_json::from_str::<Step>(both).unwrap_err().to_string().contains("both"));
        assert!(serde_json::from_str::<Step>(r#"{"id": "x"}"#).unwrap_err().to_string().contains("neither"));
        let with = r#"{"id": "x", "argv": ["docker"], "with": {"a": "b"}}"#;
        assert!(serde_json::from_str::<Step>(with).unwrap_err().to_string().contains("`with`"));
        assert!(serde_json::from_str::<Step>(r#"{"id": "x", "argv": ["docker"], "shell": true}"#).is_err());
    }
}
