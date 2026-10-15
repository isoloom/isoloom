//! The Isoloom spec: the format an author writes ([`syntax`]: parsed into types that hold no
//! invalid state, every problem coded and placed), the form the plan lowers it into ([`model`]),
//! the schema and the reference page read from the types, normalization (groups, clones, VLANs,
//! profiles, instances, defaults, the image table) and validation.
//!
//! ```
//! let spec = isoloom_spec::syntax::parse(
//!     "version: 1\nname: hello\nnetworks:\n  lab: { cidr: 10.1.0.0/24 }\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    ports: { web: { port: 80, protocol: http } }\n",
//! )?;
//! assert!(isoloom_spec::syntax::check(&spec).is_empty());
//! assert_eq!(spec.machines["web"].os.map(|os| os.to_string()).as_deref(), Some("debian@12"));
//! # Ok::<(), Vec<isoloom_spec::syntax::Problem>>(())
//! ```

pub mod appliance;
pub mod clones;
pub mod defaults;
pub mod error;
pub mod files;
pub mod groups;
pub mod home;
pub mod images;
pub mod instance;
mod load;
pub mod model;
pub mod os;
mod placeholders;
pub mod probe;
pub mod profile;
pub mod reference;
pub mod schema;
pub mod shell;
mod sizing;
pub mod syntax;
pub mod targets;
#[cfg(test)]
mod testing;
pub mod validate;
pub mod vlans;
pub mod windows;

pub use error::{LoadError, Result, Stage};
pub use load::{SPEC_FILE, SPEC_FILES, find, load, load_profiled, load_with, parse};
pub use model::{Appliance, Arch, Check, CloudServices, Declared, Dns, Expect, KNOWN_OS, Machine, Network, Port, Reach, Resources, Shape, Target, Tc, Vlan};
pub use sizing::{DEFAULT_CPUS, DEFAULT_DISK_GB, DEFAULT_MEMORY_MB, Totals, totals};
pub use syntax::Spec;
pub use targets::{derive, effective};
pub use validate::{Problem, validate, validate_files};
