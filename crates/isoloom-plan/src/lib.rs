//! The target-neutral plan of an environment: the [`Environment`] a backend renders (networks,
//! machines, their addresses, routes, volumes and set-up steps), its checks resolved to
//! positions and probes, and the requirements a backend must meet to render it.

pub mod addressing;
pub mod ansible;
pub mod boot;
pub mod checks;
pub mod environment;
pub mod hardware;
pub mod images;
pub mod ipv6;
pub mod lifecycle;
pub mod links;
pub mod machine_files;
pub mod outside;
pub mod private_dns;
pub mod process;
pub mod requirements;
pub mod routing;
pub mod secrets;

pub use environment::{Environment, Image, MachinePlan, MachineShape, NetworkPlan, PublishedPort, Route, Settings, Size, Volume, VolumeKind};
pub use lifecycle::{MachineLifecycle, Step};
pub use outside::{Join, OutsideAddress, OutsideNetwork};
pub use private_dns::PrivateDns;
pub use process::{Health, HealthTest, Process};
pub use requirements::{Capabilities, Feature, Refusal, Requirement, refusal};
pub use secrets::SecretFile;

use isoloom_spec::{model, shell};
