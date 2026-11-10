//! Isoloom's targets: the composition root above the backends. It holds the registry of every
//! target's backend, puts together the targets made of two backends (`cloud-docker`,
//! `docker-vm`, `hybrid`), and works out what spans them: an environment's files for a target
//! ([`generate`]), why a target refuses it ([`refusal()`], on one Vagrant provider or cloud
//! [`refusal_on`], every cloud's at once [`cloud_refusals`] as [`CloudRefusal`]s), the resolved
//! snapshot ([`resolved`]), the tables and diagram drawn from it ([`report`], [`graph`]), the
//! tiers ([`tiers`]) and the capability matrix ([`matrix`]). It holds no backend's logic: each backend renders its own files.
//!
//! Every function is pure (the project's own files the output copies are read only by
//! [`project_copies`]), so `isoloom check` can regenerate in memory and compare. Output lives
//! under `.isoloom/<target>/` in the project.
//!
//! # Example
//!
//! ```
//! use isoloom_spec::Target;
//! use isoloom_targets::{Lab, generate, refusal};
//!
//! let lab = Lab::new(isoloom_spec::parse(
//!     "version: 1
//! name: hello
//! networks: { lan: { cidr: 10.9.0.0/24 } }
//! machines:
//!   web: { networks: { lan: 10 }, ports: { web: 80 }, app: { image: nginx } }
//! ",
//! )?);
//! let files = generate(&lab, Target::Docker)?;
//! assert!(files.iter().any(|f| f.path == ".isoloom/docker/compose.yml"));
//! assert!(files.iter().any(|f| f.path == isoloom_targets::resolved::PATH));
//! assert!(refusal(&lab, Target::Vagrant).is_some(), "an image has no VM form");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod composite;
mod decision;
mod error;
mod generation;
pub mod gitignore;
pub mod graph;
pub mod matrix;
mod refusal;
mod registry;
pub mod report;
pub mod resolved;
pub mod tiers;

// The backends themselves are the building-block crates' port (`isoloom_render::backend`), so
// they are this crate's tests' and benches', not the supported API.
#[doc(hidden)]
pub use composite::{CloudDockerBackend, DockerVmBackend, HybridBackend};
pub use error::{Error, GenerateError, Result};
pub use generation::{default_regions, generate, generate_all, generate_instance, project_copies};
pub use isoloom_plan::{Lab, NotPossible};
pub use isoloom_render::GeneratedFile;
pub use refusal::{CloudRefusal, Clouds, cloud_refusals, refusal, refusal_on};
#[doc(hidden)]
pub use registry::backend;
