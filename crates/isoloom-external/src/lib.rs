//! The `external` backend: an environment on hosts that already exist.

pub mod external;

pub use external::generate;
use isoloom_plan::checks;
use isoloom_render::{GenerateError, GeneratedFile, OUTPUT_DIR, address, header, host_vars};
use isoloom_spec::{groups, images, model};
