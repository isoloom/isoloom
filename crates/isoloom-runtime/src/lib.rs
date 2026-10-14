//! Isoloom's runtime: the registry of running environments, the host's readiness for a
//! target, the environment report, and the lifecycle's machine-readable output (`events`: the
//! events and errors of `--json`, with an [`events::Emitter`] for tools embedding Isoloom, and
//! `tool`: a target's tool run with its output as events).

mod error;
pub mod events;
pub mod host;
pub mod registry;
pub mod report;
pub mod tool;

pub use error::{HostError, RegistryError, ReportError};

use isoloom_spec::model;
