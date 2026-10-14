//! The target-neutral plan of an environment: the [`Environment`] a backend renders (networks,
//! machines, their addresses, routes, volumes and set-up steps), its checks resolved to
//! positions and probes, and the requirements a backend must meet to render it.

pub mod addressing;
pub mod checks;
pub mod environment;
pub mod images;
pub mod lifecycle;
pub mod requirements;
pub mod routing;

pub use environment::{Environment, Image, MachinePlan, MachineShape, NetworkPlan, PublishedPort, Route, Settings, Size, Volume, VolumeKind};
pub use lifecycle::{MachineLifecycle, Step};
pub use requirements::{Capabilities, Feature, Refusal, Requirement, refusal};

use isoloom_spec::{model, shell};
