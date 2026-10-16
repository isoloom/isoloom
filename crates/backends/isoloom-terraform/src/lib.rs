//! The Terraform backends of Isoloom: an environment's plan ([`isoloom_plan::Environment`])
//! rendered as Terraform modules, with the HCL builder of `isoloom-render`.
//!
//! - **Server targets**: `proxmox` ([`ProxmoxBackend`]) and a standalone ESXi host
//!   ([`EsxiBackend`]), each environment behind a router VM of its own.
//! - **Cloud targets**: `cloud-vm` ([`CloudVmBackend`]), one cloud VM per machine and a module
//!   per [`Cloud`] ([`CloudVmModule`] renders one); `cloud-services` ([`CloudServicesBackend`]),
//!   the project's own module applied into the user's account.
//! - **The Terraform halves of the composite targets** `isoloom-targets` puts together: the
//!   Docker host on each cloud for `cloud-docker` ([`cloud_docker_modules`]), and on a Proxmox
//!   server for `docker-vm` ([`docker_vm_module`], its run manifest [`docker_vm_manifest`]).
//!
//! Every VM target sets its machines up through one core (cloud-init, `remote-exec` over SSH,
//! WinRM, cloudbase-init, the Ansible controller), so a step of the plan is written once for
//! all of them. The crate sits above `isoloom-render` and below `isoloom-targets`, and depends
//! on no other backend.
//!
//! ```
//! use isoloom_plan::{Environment, Lab};
//! use isoloom_render::backend::Backend;
//! use isoloom_spec::model::Target;
//! use isoloom_terraform::ProxmoxBackend;
//!
//! let lab = Lab::new(isoloom_spec::parse(
//!     "version: 1\nname: lab\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12, app: { steps: [{ run: web.sh }] } }\n",
//! )?);
//! let env = Environment::build(&lab.for_target(Target::Proxmox), Target::Proxmox, None);
//! let files = ProxmoxBackend.render(&env)?;
//! assert!(files.iter().any(|f| f.path == ".isoloom/proxmox/main.tf"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod checks;
mod cloud;
mod docker_host;
mod error;
mod limits;
mod machine;
mod module;
mod server;
mod target;
#[cfg(test)]
mod testing;

pub use cloud::Cloud;
pub use cloud::docker::{modules as cloud_docker_modules, refusal as cloud_docker_refusal};
pub use cloud::services::CloudServicesBackend;
pub use cloud::vm::{CloudVmBackend, CloudVmModule};
pub use error::{Error, Result};
pub use server::esxi::EsxiBackend;
pub use server::proxmox::ProxmoxBackend;
pub use server::proxmox::docker_host::{manifest as docker_vm_manifest, module as docker_vm_module};
