//! The run manifest, version 0: the contract between Isoloom and the Isothrum runner.
//!
//! Isoloom writes one `run.json` next to a target's generated files; the runner executes it and
//! reads nothing else. The manifest holds the native commands in order (argv, never a shell), the
//! documented driver actions, readiness, outputs, check runners, and the slots that carry inputs
//! and secrets (names and destinations, never values).
//!
//! The types parse, they don't validate: a [`Manifest`] that parsed holds no shell as a program,
//! no `${secrets.…}` placeholder, no absolute or climbing path, no port name with a dot, no UDP
//! port that speaks HTTP and no other version than 0. [`Manifest::validate`] then checks what
//! spans fields (step references, machines, placeholders).
//! The JSON Schema ([`schema`]) is generated from the types and committed at
//! `schemas/run-manifest-v0.schema.json`; the fixtures under `fixtures/` are shared with the
//! runner, which tests against the same files.
//!
//! The crate is pure: it reads and writes text, and touches no file, process or clock. Its enums
//! are exhaustive on purpose: the manifest is a closed, versioned contract, so a runner must handle
//! every step action, delivery and condition, and a new one fails its build instead of falling
//! into a wildcard.
//!
//! # Example
//!
//! ```
//! use isoloom_manifest::{Driver, Template};
//!
//! let manifest = isoloom_manifest::parse(include_str!("../fixtures/docker.run.json"))?;
//! assert_eq!(manifest.driver, Driver::Compose);
//! // What Isoloom writes reads back as the same manifest.
//! assert_eq!(isoloom_manifest::parse(&isoloom_manifest::to_json(&manifest)?)?, manifest);
//! // A template can't hold a secret: it reaches a tool only through its slot.
//! let refused = "--flag=${secrets.FLAG}".parse::<Template>().unwrap_err();
//! assert_eq!(refused.code(), "secret_placeholder");
//! # Ok::<(), isoloom_manifest::Error>(())
//! ```

mod argv;
mod error;
mod guest;
mod identity;
pub mod ignore;
mod lifecycle;
mod list;
mod machine;
mod manifest;
mod names;
mod outputs;
mod path;
mod port;
mod provenance;
mod schema;
mod slots;
mod step;
mod template;
mod validate;
mod walk;

pub use argv::{Argv, HostArgv, PerHostArgv, Program};
pub use error::{Error, Result};
pub use guest::Guest;
pub use identity::{
    Cloud, ComposeIdentity, Driver, Elevation, HostOs, Identity, Provider, StateOwner, Target, TerraformIdentity, Tool, VagrantIdentity, VagrantProvider,
};
pub use lifecycle::{Condition, Expiry, ExpiryUnit, Lifecycle, Park, Provision, ReadyGrammar, Recovery, Retries, Seal};
pub use list::{NonEmpty, StepSelection};
pub use machine::{Arch, Cidr, Machine, Native, Network, Resources, Role, Volume};
pub use manifest::{Manifest, parse, to_json};
pub use names::{EnvName, Identifier, LabName, Name, OsName, PinnedImage, PortName, SnakeName, StepId, ToolName, Version};
pub use outputs::{Check, CheckKind, CheckProtocol, CheckRunner, Checks, Output, OutputRole, OutputSource};
pub use path::{GuestPath, PathTemplate, RelPath};
pub use port::{Port, PortKind, Protocol, Publish, PublishMode};
pub use provenance::{Generator, GeneratorName, Instance, Lab, ManifestVersion, SUPPORTED_VERSION};
pub use schema::{SCHEMA_URL, schema};
pub use slots::{Delivery, Param, SecretSlot, SecretSource, SetBy, SlotKind, StateFileFormat, Store, TfVarShape, Transport};
pub use step::{Action, Failure, Run, Step, When};
pub use template::{Part, Placeholder, RunVar, Template};
