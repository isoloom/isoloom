//! A step as written: `reboot`, or a map with exactly one kind key, and the guard fields beside
//! it (`creates` with `run` or `command`, `unless` with `command`).

use std::fmt;

use schemars::JsonSchema;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::{
    AbsPath, CommandStep, Copy, FileStep, Firewall, GroupStep, Guard, Invocation, Package, ProjectPath, Repository, Service, Step, Template, UserStep,
};
use crate::syntax::{Code, mistake::Mistake};

/// The kind keys, as an author reads them in a list.
const KINDS: &str = "`run`, `copy`, `packages`, `playbook`, `repository`, `file`, `template`, `user`, `group`, `service`, `command` and `firewall`";

/// A step as written: one kind key, and its guard.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct StepInput {
    /// A project script (`.sh`; `.ps1` on Windows), run as root from the project folder.
    #[serde(default)]
    run: Option<ProjectPath>,
    /// With `run` or `command`: a path whose existence means the step already ran (absolute).
    #[serde(default)]
    creates: Option<AbsPath>,
    /// With `command`: a check that exits 0 when the step already ran (a text, or a list of
    /// arguments).
    #[serde(default)]
    unless: Option<Invocation>,
    /// A project file or folder copied into the machine.
    #[serde(default)]
    copy: Option<Copy>,
    /// Packages to install with the OS's installer (`apt-get`, `dnf`, `apk`; Windows features on
    /// Windows): names, or `{ name, version }` with the version the package manager writes.
    #[serde(default)]
    packages: Option<Vec<Package>>,
    /// An Ansible playbook of the project, run in the machine (`ansible-playbook -c local`).
    #[serde(default)]
    playbook: Option<ProjectPath>,
    /// A vendor's package repository (`{ name, url, key: { url, sha256 }, suite, components,
    /// priority }`), its key pinned by SHA-256. Not on Windows.
    #[serde(default)]
    repository: Option<Repository>,
    /// A path made a folder (`state: directory`), removed (`state: absent`), a link (`link`), or
    /// a download pinned by its SHA-256 (`url` and `sha256`).
    #[serde(default)]
    file: Option<FileStep>,
    /// A project file with its `{{ path }}` placeholders filled in, placed in the machine.
    #[serde(default)]
    template: Option<Template>,
    /// A local user: `{ name, uid, group, groups, home, shell, system, password }`.
    #[serde(default)]
    user: Option<UserStep>,
    /// A local group: `{ name, gid, system }`.
    #[serde(default)]
    group: Option<GroupStep>,
    /// A daemon: a unit a package installs (`nginx`), or `{ name, run, user, cwd, enabled }` for
    /// one Isoloom defines. Enabled and started on VMs; the container's process in containers.
    #[serde(default)]
    service: Option<Service>,
    /// A command run once (a text the shell runs, or a list of arguments), with exactly one of
    /// `creates` and `unless` to tell it already ran.
    #[serde(default)]
    command: Option<Invocation>,
    /// Ports opened in the OS's own firewall when one runs: `{ allow: [<port name>, <number>,
    /// <number>/udp] }`.
    #[serde(default)]
    firewall: Option<Firewall>,
}

// What the schema shows and the reader returns.
/// A step as written, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
pub(super) enum StepSchema {
    /// `reboot`: the machine restarts, and the next step runs once it is back.
    Reboot(RebootWord),
    /// A step that ensures one state.
    Map(Box<StepInput>),
}

/// The word `reboot`.
#[derive(JsonSchema)]
pub(super) enum RebootWord {
    /// Restarts the machine.
    #[serde(rename = "reboot")]
    Reboot,
}

impl<'de> Deserialize<'de> for StepSchema {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<StepSchema, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = StepSchema;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "`reboot`, or a map with one of {KINDS}")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<StepSchema, E> {
                if v == "reboot" {
                    Ok(StepSchema::Reboot(RebootWord::Reboot))
                } else {
                    Err(E::custom(format!("a step is `reboot` or a map: write `{{ run: {v} }}`")))
                }
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<StepSchema, A::Error> {
                StepInput::deserialize(de::value::MapAccessDeserializer::new(map)).map(|s| StepSchema::Map(Box::new(s)))
            }
        }
        d.deserialize_any(V)
    }
}

impl TryFrom<StepSchema> for Step {
    type Error = Mistake;

    fn try_from(written: StepSchema) -> Result<Step, Mistake> {
        match written {
            StepSchema::Reboot(RebootWord::Reboot) => Ok(Step::Reboot),
            StepSchema::Map(fields) => step(*fields),
        }
    }
}

/// The one step a map says, with its guard.
fn step(s: StepInput) -> Result<Step, Mistake> {
    let invalid = |reason: &str| Mistake::new(Code::InvalidValue, reason);
    if s.creates.is_some() && s.run.is_none() && s.command.is_none() {
        return Err(invalid("`creates` goes with `run` or `command`"));
    }
    if s.unless.is_some() && s.command.is_none() {
        return Err(invalid("`unless` goes with `command`: a `run` script says `creates`"));
    }
    // `creates` is the guard of a command, else of a script.
    let (command, creates) = match s.command {
        Some(run) => (
            Some(CommandStep {
                run,
                guard: guard(s.creates, s.unless)?,
            }),
            None,
        ),
        None => (None, s.creates),
    };
    if s.packages.as_ref().is_some_and(Vec::is_empty) {
        return Err(invalid("name the packages to install"));
    }
    let mut given: Vec<Step> = [
        s.run.map(|script| Step::Run { script, creates }),
        s.copy.map(Step::Copy),
        s.packages.map(Step::Packages),
        s.playbook.map(Step::Playbook),
        s.repository.map(Step::Repository),
        s.file.map(Step::File),
        s.template.map(Step::Template),
        s.user.map(Step::User),
        s.group.map(Step::Group),
        s.service.map(Step::Service),
        command.map(Step::Command),
        s.firewall.map(Step::Firewall),
    ]
    .into_iter()
    .flatten()
    .collect();
    match given.len() {
        1 => Ok(given.remove(0)),
        0 => Err(invalid(&format!("a step needs one of {KINDS} (or is `reboot`)"))),
        _ => Err(invalid(&format!("a step does one thing: one of {KINDS} per step"))),
    }
}

/// A command's guard: exactly one of `creates` and `unless`.
fn guard(creates: Option<AbsPath>, unless: Option<Invocation>) -> Result<Guard, Mistake> {
    match (creates, unless) {
        (Some(path), None) => Ok(Guard::Creates(path)),
        (None, Some(check)) => Ok(Guard::Unless(check)),
        (Some(_), Some(_)) => Err(Mistake::new(Code::Conflict, "a command has one guard: `creates` or `unless`")),
        (None, None) => Err(Mistake::new(Code::MissingField, "a command runs once; say how to tell it already ran")
            .fix("`creates: <path>` or `unless: <check>` beside `command`, or a `run:` script")),
    }
}
