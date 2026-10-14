//! The `external` backend: an environment on hosts that already exist, rendered from the plan.

pub mod external;

pub use external::{ExternalBackend, render};
