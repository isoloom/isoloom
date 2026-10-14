//! The Docker Compose backend: the Compose file the `docker` and `hosted` targets run (and the
//! Docker-on-a-VM targets carry), with the network appliances. It renders a planned
//! [`isoloom_plan::Environment`] through typed Compose structs ([`file`](mod@file)).

pub mod appliances;
pub mod docker;
pub mod file;
mod ipv6;
mod outside;
pub mod render;
mod sidecars;

pub use docker::{ENVIRONMENT_LABEL, MANAGED_LABEL, StartPlan, exec_runner, leaf_jobs, start_commands, start_plan};
use isoloom_render::trunks;
use isoloom_spec::{model, validate, vlans};
pub use render::{ComposeBackend, Rendered, compose_file, render, to_yaml};
