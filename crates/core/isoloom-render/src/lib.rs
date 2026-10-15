//! What every Isoloom backend renders with, so the targets render the same decisions the same
//! way. It sits between the plan (`isoloom-plan`, the facts) and the backends (Compose,
//! Kubernetes, Vagrant, Terraform, Ansible), which implement its
//! [`Backend`](backend::Backend) port.
//!
//! - **The port:** [`backend`] (the [`Backend`](backend::Backend) trait a target's renderer
//!   implements) and its [`Error`].
//! - **The builders**, one per language a target writes: [`hcl_builder`] (Terraform), [`ruby`]
//!   (a Vagrantfile's literals), [`powershell`] (a Windows machine's), [`dockerfile`] (the image
//!   built from a machine's steps) and [`inventory`] (Ansible's); shell is
//!   `isoloom_spec::shell`, below every crate.
//! - **A VM's set-up:** [`steps`] (each [`Step`](isoloom_plan::Step) as Linux shell or
//!   PowerShell), the scripts kept across restarts ([`boot`]), [`secrets`] delivered at run time,
//!   the network's [`constraints`], the [`controller`] that runs the playbooks, and the
//!   [`project`] copied in.
//! - **The network pieces:** [`router`], VLAN [`trunks`], [`ipv6`] and a VM's
//!   [`more_addresses`].
//! - **The containers' shared parts:** [`container`]; where a check reaches its host:
//!   [`checks`].
//! - **The run manifest's pieces** every target's `run.json` is built from: [`run_manifest`].
//!
//! Everything here is a pure function of the plan: no I/O, no clock, no environment.
//!
//! ```
//! use isoloom_plan::{Environment, Lab, Step};
//! use isoloom_spec::model::Target;
//!
//! let lab = Lab::parse(
//!     "version: 1\nname: demo\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\n",
//! )
//! .expect("the spec parses");
//! let env = Environment::build(&lab.for_target(Target::Vagrant), Target::Vagrant, None);
//! let ready = isoloom_render::steps::linux(&env, "web", &Step::Ready).expect("every VM gets ready");
//! assert!(ready.ends_with("echo ready > /var/lib/isoloom/ready\n"));
//! ```

pub mod backend;
pub mod boot;
pub mod checks;
pub mod constraints;
pub mod container;
pub mod controller;
pub mod dockerfile;
mod error;
pub mod hcl_builder;
pub mod inventory;
mod network;
mod output;
pub mod powershell;
pub mod project;
pub mod ruby;
pub mod run_manifest;
pub mod secrets;
pub mod steps;
#[cfg(test)]
mod testing;

pub use error::{Error, Result};
pub use network::{ipv6, more_addresses, router, trunks};
pub use output::{GeneratedFile, OUTPUT_DIR, header};
