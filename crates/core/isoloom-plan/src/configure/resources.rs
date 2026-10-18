//! The resources of a resolved recipe: each step of the model with nothing left to resolve. A
//! step that needs no resolving (a file, a user, a command) is its authored type, so the model
//! has one definition of it; the others carry what their machine's OS and ports make of them.

use std::num::NonZeroU16;

use isoloom_spec::model::Protocol;
use isoloom_spec::syntax::{AbsPath, AptWord, Argv, HttpsUrl, ProjectPath, RepositoryKey, RepositoryName, UnitName, UserName};

/// A package to install, its version pinned when given (an entry's always is, from `lock.yml`).
pub type PinnedPackage = isoloom_spec::syntax::Package;
/// A project file or folder copied as it is.
pub type CopyPlan = isoloom_spec::syntax::Copy;
/// A folder, a removal, a link or a pinned download.
pub type FilePlan = isoloom_spec::syntax::FileStep;
/// A project file whose placeholders [`super::fill`] fills in, placed in the machine.
pub type TemplatePlan = isoloom_spec::syntax::Template;
/// A local user (its literal password hashed when rendered for Linux).
pub type UserPlan = isoloom_spec::syntax::UserStep;
/// A local group.
pub type GroupPlan = isoloom_spec::syntax::GroupStep;
/// A command run once, behind its guard.
pub type CommandPlan = isoloom_spec::syntax::CommandStep;

/// One resource of a recipe, in the order the recipe lists it: no renderer reorders, groups or
/// parallelizes them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Resource {
    /// A vendor's package repository.
    Repository(RepositoryPlan),
    /// Packages, installed together.
    Packages(Vec<PinnedPackage>),
    /// A project file or folder.
    Copy(CopyPlan),
    /// A folder, a removal, a link or a download.
    File(FilePlan),
    /// A template, filled in at generation.
    Template(TemplatePlan),
    /// A user.
    User(UserPlan),
    /// A group.
    Group(GroupPlan),
    /// A daemon.
    Service(ServicePlan),
    /// A command, behind its guard.
    Command(CommandPlan),
    /// Ports opened in the OS's firewall.
    Firewall(Vec<PortSpec>),
    /// A project script.
    Run(RunPlan),
    /// A restart.
    Reboot,
    /// An Ansible playbook of the project.
    Playbook(ProjectPath),
}

impl Resource {
    /// The step kind it comes from, as written (`repository`, `run`).
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Resource::Repository(_) => "repository",
            Resource::Packages(_) => "packages",
            Resource::Copy(_) => "copy",
            Resource::File(_) => "file",
            Resource::Template(_) => "template",
            Resource::User(_) => "user",
            Resource::Group(_) => "group",
            Resource::Service(_) => "service",
            Resource::Command(_) => "command",
            Resource::Firewall(_) => "firewall",
            Resource::Run(_) => "run",
            Resource::Reboot => "reboot",
            Resource::Playbook(_) => "playbook",
        }
    }
}

/// The package manager a repository is added to: the OS family's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RepositoryKind {
    /// A deb822 source and its keyring (Debian's family).
    Apt,
    /// A `.repo` file and its key (Red Hat's family).
    Rpm,
    /// A `repositories` line and its key (Alpine).
    Apk,
}

/// A repository, its suite and components resolved (apt's) and its key pinned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryPlan {
    /// Its name: it names the source and key files.
    pub name: RepositoryName,
    /// The package manager it is added to.
    pub kind: RepositoryKind,
    /// Its address.
    pub url: HttpsUrl,
    /// Its signing key, pinned by SHA-256.
    pub key: RepositoryKey,
    /// The suite (apt: the step's, else the OS's codename).
    pub suite: Option<AptWord>,
    /// The components (apt: the step's, else `main`; empty for the others).
    pub components: Vec<AptWord>,
    /// Its priority over the OS's own packages.
    pub priority: Option<i32>,
}

/// A daemon, its unit resolved for the family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServicePlan {
    /// The unit, as the family names it (`ssh` on Debian for the table's `ssh`, `sshd` on Rocky).
    pub unit: UnitName,
    /// Where its unit comes from.
    pub source: ServiceSource,
    /// Started at boot and now (else disabled and stopped).
    pub enabled: bool,
}

/// Where a service's unit comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ServiceSource {
    /// A package installs it.
    Package,
    /// Isoloom defines it from the step's `run`.
    Defined {
        /// What runs, in the foreground.
        run: Argv,
        /// The user it runs as (root when `None`).
        user: Option<UserName>,
        /// The folder it runs in.
        cwd: Option<AbsPath>,
    },
}

/// A port, by number and transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PortSpec {
    /// The number.
    pub port: NonZeroU16,
    /// The transport.
    pub protocol: Protocol,
}

/// A project script, and the shell that runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPlan {
    /// The script.
    pub script: ProjectPath,
    /// A path whose existence means it already ran.
    pub creates: Option<AbsPath>,
    /// What runs it.
    pub shell: Shell,
}

/// What runs a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Shell {
    /// `sh` (Linux).
    Posix,
    /// PowerShell (Windows).
    PowerShell,
}
