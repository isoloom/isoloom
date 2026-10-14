//! Isoloom's runtime state: the registry of running environments, the host's readiness for a
//! target, and the environment report.

pub mod host;
pub mod registry;
pub mod report;

use isoloom_spec::model;
