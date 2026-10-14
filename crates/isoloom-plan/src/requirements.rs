//! What an environment asks of a backend, and what a backend offers. A backend refuses an
//! environment when it lacks the capability one of its requirements needs: the refusal comes
//! from the plan, without generating anything.

use std::collections::BTreeSet;
use std::fmt;

use isoloom_spec::model::Spec;

use crate::checks::{self, Position, Probe};

/// A capability a backend may have: the kind of a [`Requirement`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Feature {
    /// Environment-level provisioning (`provision:`), run by a controller.
    EnvironmentProvisioning,
    /// Checks that are Ansible playbooks (run from a controller).
    PlaybookChecks,
    /// `exec` checks from a position with no container of its own.
    ExecWithoutContainer,
    /// Network appliances (`docker.appliance`), wired the way Docker attaches networks.
    NetworkAppliances,
}

/// One thing the environment needs from the backend that renders it, with what in the spec
/// asks for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requirement {
    /// The spec has `provision:` steps.
    EnvironmentProvisioning,
    /// A check is an Ansible playbook.
    PlaybookChecks,
    /// An `exec` check stands where no container of the environment runs.
    ExecWithoutContainer {
        /// The check's name.
        check: String,
    },
    /// A machine is a network appliance.
    NetworkAppliance {
        /// The machine.
        machine: String,
    },
}

impl Requirement {
    /// The capability this requirement needs.
    pub fn feature(&self) -> Feature {
        match self {
            Requirement::EnvironmentProvisioning => Feature::EnvironmentProvisioning,
            Requirement::PlaybookChecks => Feature::PlaybookChecks,
            Requirement::ExecWithoutContainer { .. } => Feature::ExecWithoutContainer,
            Requirement::NetworkAppliance { .. } => Feature::NetworkAppliances,
        }
    }
}

impl fmt::Display for Requirement {
    /// Why a backend without the capability refuses the environment, in the user's terms.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Requirement::EnvironmentProvisioning => f.write_str("environment-level provisioning (`provision:`) runs on VM targets for now"),
            Requirement::PlaybookChecks => f.write_str("Ansible checks (.yml) run on VM targets for now"),
            Requirement::ExecWithoutContainer { check } => {
                write!(f, "`exec` check \"{check}\" needs `from:` a machine with its own container (it runs inside it)")
            }
            Requirement::NetworkAppliance { machine } => write!(
                f,
                "machine `{machine}` is a network appliance (`docker.appliance`), which runs on the Docker targets"
            ),
        }
    }
}

/// What a backend can render.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capabilities {
    features: BTreeSet<Feature>,
}

impl Capabilities {
    /// A backend with these capabilities.
    pub fn of(features: impl IntoIterator<Item = Feature>) -> Capabilities {
        Capabilities {
            features: features.into_iter().collect(),
        }
    }

    /// Whether the backend has `feature`.
    pub fn has(&self, feature: Feature) -> bool {
        self.features.contains(&feature)
    }
}

/// Why a backend won't render an environment: the first requirement it can't meet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The requirement the backend lacks the capability for.
    pub requirement: Requirement,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.requirement.fmt(f)
    }
}

/// The first of `requirements` that `caps` can't meet, in the order the plan lists them.
pub fn refusal(requirements: &[Requirement], caps: &Capabilities) -> Option<Refusal> {
    requirements.iter().find(|r| !caps.has(r.feature())).map(|r| Refusal { requirement: r.clone() })
}

/// Everything `spec` requires, most fundamental first.
pub fn of(spec: &Spec) -> Vec<Requirement> {
    let mut out = Vec::new();
    if !spec.provision.is_empty() {
        out.push(Requirement::EnvironmentProvisioning);
    }
    out.extend(container_checks(spec));
    out.extend(
        spec.machines
            .iter()
            .filter(|(_, m)| m.docker.as_ref().is_some_and(|d| d.appliance.is_some()))
            .map(|(name, _)| Requirement::NetworkAppliance { machine: name.clone() }),
    );
    out
}

/// The checks' requirements beyond probing from a container: playbooks, and `exec` checks that
/// would have no container to run in. At most one of them: the first, as a container output
/// reports it.
pub fn container_checks(spec: &Spec) -> Option<Requirement> {
    if spec.checks.iter().any(isoloom_spec::Check::is_playbook) {
        return Some(Requirement::PlaybookChecks);
    }
    checks::plan(spec)
        .into_iter()
        .filter(|c| matches!(c.probe, Probe::Exec { .. }))
        .find(|c| match &c.position {
            Position::Machine(m) => !spec.machines.get(m).is_some_and(|m| m.docker.is_some() && !m.supplied),
            Position::Networks => true,
        })
        .map(|c| Requirement::ExecWithoutContainer { check: c.name })
}
