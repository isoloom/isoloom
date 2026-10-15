//! What a run gives back (the entry URL, addresses, credentials), and the checks that prove it
//! behaves as its spec says.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::names::{Identifier, Name, SnakeName};
use crate::step::Step;
use crate::template::Template;

/// What the embedder does with an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OutputRole {
    /// Where to start: the Open button.
    Entry,
    /// The lab host's login.
    Ssh,
    /// An address.
    Address,
    /// A credential.
    Credential,
    /// Anything else worth showing.
    Info,
}

/// Where an output's value comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutputSource {
    /// The host port a machine's port got.
    PublishedPort {
        /// The machine.
        machine: Name,
        /// The port's name.
        port: Name,
    },
    /// A Terraform output of the module.
    TerraformOutput {
        /// The output's name.
        name: Identifier,
        /// The value when the state has none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
    /// A value known when the files were generated.
    Static {
        /// The value.
        value: String,
    },
}

/// A value the run gives back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Output {
    /// Its name (`${outputs.<name>}`).
    pub name: SnakeName,
    /// What it is, for the reader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What the embedder does with it.
    pub role: OutputRole,
    /// Masked in every event and log line, and never substituted in the message.
    pub sensitive: bool,
    /// How the value is shown, `${value}` being the raw one (`http://127.0.0.1:${value}/`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<Template>,
    /// Where the value comes from.
    pub from: OutputSource,
}

/// The line protocol check runners print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CheckProtocol {
    /// `isoloom-check: PASS <name>`, `isoloom-check: FAIL <name>: <reason>`,
    /// `isoloom-check: END <p> passed, <f> failed`, matched anywhere in a line (a Vagrant or
    /// Compose prefix before it); the runner exits 0 only without a failure.
    #[serde(rename = "isoloom-check/1")]
    V1,
}

/// What a check probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CheckKind {
    /// An HTTP request.
    Http,
    /// A TCP connection.
    Tcp,
    /// A script from the project.
    Script,
    /// A command inside a machine.
    Exec,
    /// An Ansible playbook from the project, run from the controller.
    Playbook,
}

/// One check a runner reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// The name the runner reports it by.
    pub name: String,
    /// What it probes.
    pub kind: CheckKind,
    /// How long the runner retries it before it fails (the spec's `wait`), in seconds.
    pub wait_s: u32,
    /// Derived from the spec (reachability, no internet); skipped with ISOLOOM_DERIVED=0.
    pub derived: bool,
}

/// One check runner: where it runs, what it reports, the step that runs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CheckRunner {
    /// Its id.
    pub id: Name,
    /// Where it runs, for the report: `from the environment's networks`, `from web`, `inside web`.
    pub from: String,
    /// What it reports, in order. A check its runner never reported counts as failed.
    pub checks: Vec<Check>,
    /// Its `timeout_s` covers every check's `wait_s` plus a margin: a runner never stops before
    /// its slowest check could pass.
    pub step: Step,
    /// Clean-up after the runner ran out of time (its one-off container outlives the killed CLI).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_timeout: Vec<Step>,
}

/// The lab's checks, run by the runners Isoloom generated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Checks {
    /// The line protocol the runners print.
    pub protocol: CheckProtocol,
    /// Run before the runners (the access stand-ins; waking the controller).
    #[serde(default)]
    pub setup: Vec<Step>,
    /// The runners, in order.
    pub runners: Vec<CheckRunner>,
    /// Run after the runners, whatever happened.
    #[serde(default)]
    pub teardown: Vec<Step>,
}
