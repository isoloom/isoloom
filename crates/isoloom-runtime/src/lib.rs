//! Isoloom's runtime state: the registry of running environments, the host's readiness for a
//! target, and the environment report.

mod error;
pub mod host;
pub mod registry;
pub mod report;

pub use error::{HostError, RegistryError, ReportError};

use isoloom_spec::model;
