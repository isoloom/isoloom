//! The target-neutral plan of a lab: the authored spec lowered for a target ([`Lab`], [`lower`]),
//! then planned into the [`Environment`] a backend renders (networks, machines, their addresses,
//! routes, volumes and set-up steps), its checks resolved to positions and probes, and the
//! requirements a backend must meet to render it.
//!
//! It sits between the spec (`isoloom-spec`, below) and the rendering layer and backends
//! (`isoloom-render` and up), and is a functional core: every function here is pure over the
//! spec, with no I/O, clock or environment, except [`Lab::problems_with_files`], which looks at
//! the project folder it is given.
//!
//! # Example
//!
//! ```
//! use isoloom_plan::{Environment, Lab, MachineShape};
//! use isoloom_spec::model::Target;
//!
//! let lab = Lab::new(isoloom_spec::parse(
//!     "version: 1
//! name: hello
//! networks: { lan: { cidr: 10.9.0.0/24 } }
//! machines:
//!   web: { networks: { lan: 10 }, ports: { site: 80 }, app: { image: nginx } }
//! ",
//! )?);
//! assert!(lab.problems().is_empty());
//! assert!(lab.possible().contains(&Target::Docker));
//! let env = Environment::build(&lab.for_target(Target::Docker), Target::Docker, None);
//! let web = env.machines.get("web").ok_or("web is planned")?;
//! assert_eq!(web.shape, MachineShape::Container);
//! assert_eq!(web.addresses.get("lan").map(ToString::to_string).as_deref(), Some("10.9.0.10"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod addressing;
pub mod ansible;
pub mod boot;
pub mod checks;
pub mod environment;
pub mod hardware;
pub mod images;
pub mod ipv6;
pub mod lab;
pub mod lifecycle;
pub mod links;
pub mod lower;
pub mod machine_files;
pub mod more_addresses;
pub mod outside;
pub mod private_dns;
pub mod process;
pub mod requirements;
pub mod routing;
pub mod secrets;
pub mod storage;
#[cfg(test)]
mod testing;

pub use environment::{
    Build, Controller, Environment, Image, MachinePlan, MachineShape, NetworkPlan, PublishedPort, Route, Settings, Size, Volume, VolumeKind,
};
pub use lab::{Lab, problems_in};
pub use lifecycle::{MachineLifecycle, Step};
pub use outside::{Join, OutsideAddress, OutsideNetwork};
pub use private_dns::PrivateDns;
pub use process::{Health, HealthTest, Process};
pub use requirements::{Capabilities, Feature, Refusal, Requirement, refusal};
pub use secrets::{SecretFile, ValueSource};
pub use storage::{DataDisk, Lifetime};
