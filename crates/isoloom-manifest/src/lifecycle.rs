//! The operations a run goes through (up, down, park, resume, provision, status), when it counts
//! as ready, and how a start that tripped over leftovers is retried.

use std::borrow::Cow;
use std::num::NonZeroU32;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ManifestError;
use crate::list::NonEmpty;
use crate::names::{Name, SnakeName, StepId};
use crate::step::Step;
use crate::template::Template;

/// How a run can be parked: absent, it can't (Terraform runs are created or destroyed); a mode
/// absent inside it is refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Park {
    /// Freeze the machines, keeping their memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pause: Option<Vec<Step>>,
    /// Power the machines off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shutdown: Option<Vec<Step>>,
}

/// Set-up again on running machines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provision {
    /// Every machine.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all: Vec<Step>,
    /// One machine, `${machine}` bound to it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub machine: Vec<Step>,
}

/// The operations, as ordered steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lifecycle {
    /// Creates and starts the run.
    pub up: NonEmpty<Step>,
    /// Removes everything the run created.
    pub down: NonEmpty<Step>,
    /// Parks a running run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub park: Option<Park>,
    /// Brings parked machines back as they were. Never re-creates a container nor re-provisions a
    /// VM, so it needs no input again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resume: Vec<Step>,
    /// Set-up again on running machines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provision: Option<Provision>,
    /// The read-only probe the driver parses into machine states (an argv) or reads from files
    /// (an action). Never elevated, always time-bounded.
    pub status: Step,
}

/// The grammar of a ready file: its first line is `ready`, `failed: <step>` or `running: <step>`;
/// anything else (or no file) means still booting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ReadyGrammar {
    /// Version 1 of the grammar.
    #[serde(rename = "isoloom-ready/1")]
    V1,
}

/// A condition that must hold, once `lifecycle.up` succeeded, before the run counts as ready.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    /// An up step exited 0.
    StepOk {
        /// The step.
        step: StepId,
        /// What its success means, for the reader.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        means: Option<String>,
    },
    /// These machines are running.
    MachinesRunning {
        /// The machines.
        machines: Vec<Name>,
    },
    /// A file on the lab host says the bootstrap finished, read over SSH.
    ReadyFile {
        /// The lab host.
        host: Template,
        /// The SSH user.
        user: Template,
        /// The ready file's path on the host.
        path: Template,
        /// The bootstrap's log, whose tail explains a failure.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        log: Option<Template>,
        /// The file's grammar.
        grammar: ReadyGrammar,
        /// A jump host (`user@host`), when the lab host sits behind one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        via: Option<Template>,
        /// How long the bootstrap may take, in seconds.
        timeout_s: NonZeroU32,
        /// How long the host may stay unreachable before the run fails, in seconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unreachable_timeout_s: Option<NonZeroU32>,
        /// Seconds between two reads.
        interval_s: NonZeroU32,
    },
    /// A TCP port accepts connections.
    Tcp {
        /// The address.
        address: Template,
        /// The port.
        port: Template,
        /// How long to wait, in seconds.
        timeout_s: NonZeroU32,
    },
}

/// How many more tries a failed start gets (0 to 3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Retries(u8);

impl Retries {
    /// The count, when it is 0 to 3.
    pub fn new(count: u8) -> Result<Retries, ManifestError> {
        if count <= 3 {
            Ok(Retries(count))
        } else {
            Err(ManifestError::OutOfRange {
                what: "a retry count",
                value: count.into(),
                min: 0,
                max: 3,
            })
        }
    }

    /// The count.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl Serialize for Retries {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for Retries {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Retries, D::Error> {
        Retries::new(u8::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Retries {
    fn schema_name() -> Cow<'static, str> {
        "Retries".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "integer", "minimum": 0, "maximum": 3 })
    }
}

/// A failed `up` that tripped over a previous run's leftovers, and how to clear them before one
/// more try. A genuine failure is never retried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recovery {
    /// Case-insensitive substrings of the failed step's output that mean leftovers, not a lab
    /// failure.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale_markers: Vec<String>,
    /// What clears the leftovers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,
    /// More tries after clearing.
    #[serde(default)]
    pub retries: Retries,
    /// Terraform only: more applies after a failed one (a module that converges on a second
    /// apply).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply_retries: Option<Retries>,
}

/// How a run's expiry is passed to the module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExpiryUnit {
    /// Seconds since the Unix epoch.
    UnixSeconds,
    /// Minutes from the apply.
    MinutesFromNow,
}

/// Runs that stop themselves (cloud auto-stop): the runner computes the expiry when it applies,
/// passes it to the module, records it, and lists the run as expired after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Expiry {
    /// The Terraform variable it goes in.
    pub tf_var: SnakeName,
    /// Its unit.
    pub unit: ExpiryUnit,
    /// The run's life when the embedder sets none, in minutes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_minutes: Option<u32>,
}
