//! The run manifest, version 1: the contract between Isoloom and a runner.
//!
//! Isoloom writes one `run.json` next to a target's generated files; the runner executes it and
//! reads nothing else. The manifest holds the native commands in order (argv, never a shell), the
//! documented driver actions, readiness, outputs, check runners, and the slots that carry inputs
//! and secrets (names and destinations, never values).
//!
//! The types parse, they don't validate: a [`Manifest`] that parsed holds no shell as a program,
//! no `${secrets.…}` placeholder, no absolute or climbing path, no port name with a dot, no UDP
//! port that speaks HTTP and no other version than 1. [`Manifest::validate`] then checks what
//! spans fields (step references, machines, placeholders).
//! The JSON Schema ([`schema()`]) is generated from the types and committed at
//! `schemas/run-manifest-v1.schema.json`, published at [`SCHEMA_URL`]; runners can test against
//! the fixtures under `fixtures/`. [`CheckLine::parse`] reads the lines a check runner prints, and
//! [`contract`] holds the names the generated files and a runner share outside `run.json`.
//!
//! # Compatibility
//!
//! Version 1 is stable. Within a version, changes are additive: a new optional field comes in a
//! minor release and the version stays. Removing or renaming a field, changing what a field holds
//! or means, making a field required, or adding a value a runner must act on (a step action, a
//! delivery, a condition) bumps the version. A reader is strict: it refuses a field or value it
//! doesn't know, so a manifest that uses a field added in a minor release needs a runner built
//! with that release. The policy is [`COMPATIBILITY`], which the schema states too.
//!
//! [`COMPATIBILITY`] is the document's: what a runner reading `run.json` relies on. The types
//! mirror the document and are exhaustive on purpose, enums and structs alike: the manifest is a
//! closed, versioned contract, so a runner must handle every step action, delivery and
//! condition, and a new one fails its build instead of falling into a wildcard. A new optional
//! field of the document is a new public field of its struct, in a minor release: code that
//! builds one of these structs by hand, or destructures one without `..`, follows it then. Read
//! a manifest with [`parse`], match its structs with `..`.
//!
//! The crate is pure: it reads and writes text, and touches no file, process or clock.
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
//! let refused = "--token=${secrets.TOKEN}".parse::<Template>().unwrap_err();
//! assert_eq!(refused.code(), "secret_placeholder");
//! # Ok::<(), isoloom_manifest::Error>(())
//! ```

pub mod access;
mod argv;
mod check_line;
mod console;
pub mod contract;
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
mod ready_line;
mod schema;
mod slots;
mod step;
mod template;
mod validate;
mod walk;

pub use argv::{Argv, HostArgv, PerHostArgv, Program};
pub use check_line::CheckLine;
pub use console::{Console, ConsoleAccount, ConsoleProtocol, ConsoleSource};
pub use error::{Error, Result};
pub use guest::Guest;
pub use identity::{
    Cloud, ComposeIdentity, Driver, Elevation, GoneError, HostOs, Identity, Provider, StateOwner, Target, TerraformIdentity, Tool, VagrantIdentity,
    VagrantProvider,
};
pub use lifecycle::{Condition, Expiry, ExpiryUnit, Lifecycle, Park, Provision, ReadyGrammar, Recovery, Retries, Seal};
pub use list::{NonEmpty, StepSelection};
pub use machine::{Arch, Cidr, Machine, Native, Network, Resources, Role, Volume};
pub use manifest::{Manifest, parse, to_json};
pub use names::{
    EnvName, EnvironmentName, GoneText, Identifier, KEBAB_MAX, Name, OsName, PinnedImage, PortName, SnakeName, StepId, ToolName, Version, is_kebab,
};
pub use outputs::{Check, CheckKind, CheckProtocol, CheckRunner, Checks, Output, OutputRole, OutputSource};
pub use path::{GuestPath, PathTemplate, RelPath};
pub use port::{Port, PortKind, Protocol, Publish, PublishMode};
pub use provenance::{EnvironmentRef, Generator, GeneratorName, Instance, ManifestVersion};
pub use ready_line::ReadyLine;
pub use schema::{COMPATIBILITY, SCHEMA_URL, schema};
pub use slots::{Delivery, Param, SecretSlot, SecretSource, SetBy, SlotKind, StateFileFormat, Store, TfVarShape, Transport};
pub use step::{Action, Failure, Run, Step, When};
pub use template::{Part, Placeholder, RunVar, Template};
