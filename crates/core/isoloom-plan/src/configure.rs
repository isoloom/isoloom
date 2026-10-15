//! The configuration model resolved (PLAN §3, design `plan/phase4/design.md` §1–§3): what a
//! machine's recipe is once its OS, its ports and the daemon table have been read, with nothing
//! left to resolve. The CM renderers (Phase 4.2 on: the shell and PowerShell libraries, the
//! Dockerfile, cloud-init, Ansible) render a [`Recipe`]; they never read the authored steps.
//!
//! - [`recipe`]: a machine's steps resolved for its family ([`Family`]), or the steps the family
//!   can't carry at all ([`Unportable`]: a repository on Windows, a Windows file's ACL).
//! - [`ContainerProcess`]: what its container runs (design §3), or why it can't ([`NoProcess`]).
//! - [`DaemonTable`]: the daemons' units and foreground forms; [`unstarted`]: the daemons a
//!   recipe installs and no step starts.
//! - [`fill`]: a template's placeholders filled in for one machine on one target.
//! - [`Configure`] and [`Handoff`]: the CM tools, each target's default, and how the infrastructure tool
//!   reaches the rendered configuration.
//!
//! All of it is pure: no I/O, no clock, no environment.

mod daemons;
mod family;
mod process;
mod recipe;
mod resources;
mod template;
mod tools;
mod unstarted;

pub use daemons::{Daemon, DaemonEntry, DaemonTable, FamilyDaemon, LogLink, Stream};
pub use family::{Family, codename};
pub use process::{ContainerProcess, NoProcess};
pub use recipe::{Recipe, Unportable, recipe};
pub use resources::{
    CommandPlan, CopyPlan, FilePlan, GroupPlan, PinnedPackage, PortSpec, RepositoryKind, RepositoryPlan, Resource, RunPlan, ServicePlan, ServiceSource, Shell,
    TemplatePlan, UserPlan,
};
pub use template::{Context, fill};
pub use tools::{Configure, Handoff, OutPath};
pub use unstarted::{Unstarted, unstarted};
