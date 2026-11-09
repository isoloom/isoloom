//! The configuration model resolved: what a
//! machine's recipe is once its OS, its ports and the daemon table have been read, with nothing
//! left to resolve. The CM renderers (`isoloom-render::configure`: the shell and PowerShell
//! libraries, the
//! Dockerfile, cloud-init, Ansible) render a [`Recipe`]; they never read the authored steps.
//!
//! - [`recipe`]: a machine's steps resolved for its family ([`Family`]), or the steps the family
//!   can't carry at all ([`Unportable`]: a repository on Windows, a Windows file's ACL).
//! - [`ContainerProcess`]: what its container runs, or why it can't ([`NoProcess`]).
//! - [`DaemonTable`]: the daemons' units and foreground forms; [`unstarted`]: the daemons a
//!   recipe installs and no step starts.
//! - [`fill`]: a template's placeholders filled in for one machine on one target.
//!
//! All of it is pure: no I/O, no clock, no environment.

mod daemons;
mod family;
mod process;
mod recipe;
mod resources;
mod template;
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
pub use unstarted::{Unstarted, unstarted};
