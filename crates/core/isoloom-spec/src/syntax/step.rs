//! The configuration resource model: an app's `steps`, each one state it ensures behind a guard,
//! kept in list order by every renderer. The approved five (`run`, `copy`, `packages`, `reboot`,
//! `playbook`) and eight more: `repository`, `file`, `template`, `user`, `group`, `service`,
//! `command` (with its required guard) and `firewall` (allow rules only).
//!
//! A step is `reboot`, or a map with exactly one kind key; fields such as `creates` and `unless`
//! sit beside that key. Every step parses into types that hold no invalid value (newtypes for
//! names, paths, URLs and checksums), and a mistake the design names (a command without its
//! guard, a download without its checksum, a secret password) is reported with its code, field
//! and fix.

/// The JSON Schema of `$ty` is that of `$written`, the shape it is written in.
macro_rules! schema_as {
    ($ty:ident, $written:ident) => {
        impl schemars::JsonSchema for $ty {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($ty).into()
            }

            fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
                <$written as schemars::JsonSchema>::json_schema(g)
            }
        }
    };
}

mod accounts;
mod files;
mod input;
mod services;
mod software;
mod words;

pub use accounts::{GroupStep, Password, UserStep};
pub use files::{Copy, FileState, FileStep, Template};
pub use services::{Argv, CommandStep, Firewall, FirewallPort, Guard, Invocation, Service};
pub use software::{Package, Repository, RepositoryKey};
pub use words::{AbsPath, AptWord, GroupName, HttpsUrl, PackageName, PackageVersion, ProjectPath, RepositoryName, Sha256, UnitName, UserName};

use serde::Deserialize;

/// One step of the portable recipe. Steps install and configure: they run as root from the
/// project copy at `/opt/isoloom`, with no network to the other machines and no inputs or
/// secrets (on containers they run at image build time). What needs those goes in `setup`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "input::StepSchema")]
#[non_exhaustive]
pub enum Step {
    /// A project script (`.sh`, or `.ps1` on Windows), skipped when `creates` exists.
    Run {
        /// The script.
        script: ProjectPath,
        /// A path whose existence means the step already ran.
        creates: Option<AbsPath>,
    },
    /// A project file or folder copied into the machine.
    Copy(Copy),
    /// Packages installed with the OS's own installer (Windows features on Windows), never
    /// empty.
    Packages(Vec<Package>),
    /// The machine restarts, and the next step runs once it is back (no container form).
    Reboot,
    /// An Ansible playbook of the project, run in the machine (no container form in 1.0).
    Playbook(ProjectPath),
    /// A vendor's package repository, its key pinned.
    Repository(Repository),
    /// A folder, a removal, a link or a pinned download.
    File(FileStep),
    /// A project file with its placeholders filled in.
    Template(Template),
    /// A local user.
    User(UserStep),
    /// A local group.
    Group(GroupStep),
    /// A daemon: enabled and started on VMs, the container's process in containers.
    Service(Service),
    /// A command run once, behind its guard.
    Command(CommandStep),
    /// Ports opened in the OS's own firewall.
    Firewall(Firewall),
}

impl Step {
    /// The step's kind, as its key is written (`run`, `copy`, `reboot`).
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Step::Run { .. } => "run",
            Step::Copy(_) => "copy",
            Step::Packages(_) => "packages",
            Step::Reboot => "reboot",
            Step::Playbook(_) => "playbook",
            Step::Repository(_) => "repository",
            Step::File(_) => "file",
            Step::Template(_) => "template",
            Step::User(_) => "user",
            Step::Group(_) => "group",
            Step::Service(_) => "service",
            Step::Command(_) => "command",
            Step::Firewall(_) => "firewall",
        }
    }

    /// Why the step has no container form, when it has none.
    #[must_use]
    pub fn container_refusal(&self) -> Option<&'static str> {
        match self {
            Step::Reboot => Some("a reboot needs a machine with its own kernel and boot: a container has neither"),
            Step::Playbook(_) => Some("a playbook step runs in a VM only in 1.0"),
            Step::Run { script, .. } if has_extension(script.as_str(), "ps1") => Some("a PowerShell script means Windows, which has no container form in 1.0"),
            _ => None,
        }
    }
}

/// Whether `path` ends in `.<extension>`, in any case.
pub(crate) fn has_extension(path: &str, extension: &str) -> bool {
    std::path::Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case(extension))
}

impl schemars::JsonSchema for Step {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Step".into()
    }

    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <input::StepSchema as schemars::JsonSchema>::json_schema(g)
    }
}

#[cfg(test)]
mod tests;
