//! Isoloom's Ansible backend: the `external` target, an environment on hosts that already
//! exist (a hand-built Proxmox or ESXi set of VMs, a home lab, hardware). Isoloom creates
//! nothing there; it provisions, checks and reaches the hosts over SSH at the addresses the
//! spec gives (`overrides.external.machines.<m>.ssh`), from the files it writes in
//! `.isoloom/external/`:
//!
//! - `inventory.ini`: every machine with its `external` address, user, port and key, in the
//!   `linux` / `windows` groups and the inventory groups, for `provision:` playbooks run from
//!   there ([`isoloom_render::inventory`], each host reached as its `external:` says).
//! - `site.yml` and `roles/isoloom_machine_<m>/` (`-` in `<m>` as `_`): each configured machine's recipe as a play and
//!   a role of native tasks ([`configure`]), and `requirements.yml` when a role needs a pinned
//!   collection.
//! - `checks/<machine>.sh`: the check runner of each machine that has an address (declared and
//!   derived probes; scripts need the project on the machine and are left out, with a note).
//!
//! Networks and `reach` are expected behaviour there (Isoloom adds no router), which the derived
//! checks verify.
//!
//! It sits with the other backends, above the rendering layer (`isoloom-render`) and below the
//! targets' registry (`isoloom-targets`), and depends on no other backend: [`ExternalBackend`]
//! renders a planned [`Environment`](isoloom_plan::Environment), purely, into files.
//!
//! # Example
//!
//! ```
//! use isoloom_ansible::ExternalBackend;
//! use isoloom_plan::{Environment, Lab};
//! use isoloom_render::backend::Backend;
//! use isoloom_spec::model::Target;
//!
//! let lab = Lab::new(isoloom_spec::parse(
//!     "version: 1
//! name: home-lab
//! networks: { lan: { cidr: 10.9.0.0/24 } }
//! machines:
//!   web: { networks: { lan: 5 }, os: debian@12 }
//! overrides:
//!   external: { machines: { web: { ssh: { address: 192.168.1.20 } } } }
//! ",
//! )?);
//! let env = Environment::build(&lab.for_target(Target::External), Target::External, None);
//! let files = ExternalBackend.render(&env)?;
//! let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
//! assert_eq!(paths, [".isoloom/external/inventory.ini"]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod backend;
mod checks;
pub mod configure;
mod error;
#[cfg(test)]
mod testing;

pub use backend::ExternalBackend;
pub use error::{Error, Result};
