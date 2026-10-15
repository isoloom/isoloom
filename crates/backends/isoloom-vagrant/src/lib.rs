//! Isoloom's Vagrant backend: the `vagrant` target (one VM per machine, on every provider), and
//! the Vagrant sides of `docker-vm` (Docker on one local VM) and `hybrid` (containers and VMs
//! side by side), which `isoloom-targets` puts together with their Compose files. Every
//! Vagrantfile is written with one Ruby builder, so no value can end a string or interpolate,
//! and the statements every Vagrantfile shares are written once.
//!
//! It sits with the other backends, above the rendering layer (`isoloom-render`) and below the
//! targets' registry (`isoloom-targets`), and depends on no other backend: [`VagrantBackend`]
//! renders a planned [`Environment`](isoloom_plan::Environment), purely, into files;
//! [`docker_vm`] and [`hybrid`] write those targets' Vagrant sides; [`provider_refusal`] and
//! [`disk_refusal`] say what a provider can't run.
//!
//! # Example
//!
//! ```
//! use isoloom_plan::Environment;
//! use isoloom_render::backend::Backend;
//! use isoloom_spec::model::Target;
//! use isoloom_vagrant::{VagrantBackend, provider_refusal};
//!
//! let spec = isoloom_spec::parse(
//!     "version: 1
//! name: hello
//! networks: { lan: { cidr: 10.9.0.0/24 } }
//! machines:
//!   web: { networks: { lan: 10 }, ports: [{ port: 80 }], vm: { os: debian@12 } }
//! ",
//! )?;
//! let env = Environment::build(&spec, Target::Vagrant, None);
//! let files = VagrantBackend.render(&env)?;
//! let vagrantfile = files.iter().find(|f| f.path == ".isoloom/vagrant/Vagrantfile").ok_or("no Vagrantfile")?;
//! assert!(vagrantfile.contents.contains("config.vm.define \"web\" do |m|"));
//! assert_eq!(provider_refusal(&spec, "virtualbox"), None);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod backend;
mod docker_host;
pub mod docker_vm;
mod error;
pub mod hybrid;
mod ruby;
mod stable_hash;
#[cfg(test)]
mod testing;
mod vagrantfile;
mod vms;

pub use backend::VagrantBackend;
pub use error::{Error, Result};
pub use vms::provider::{disk_refusal, provider_refusal};
