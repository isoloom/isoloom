//! What an environment asks of a backend, and what a backend offers. A backend refuses an
//! environment when it lacks the capability one of its requirements needs: the refusal comes
//! from the plan, without generating anything.

use std::collections::BTreeSet;
use std::fmt;

use isoloom_spec::model::Spec;

use crate::checks::{self, Position, Probe};
use crate::environment::MachineShape;

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
    /// Networks routed by a machine of the environment (a network's `gateway`).
    Gateways,
    /// One shared volume mounted by containers and VMs alike.
    VolumesAcrossShapes,
    /// A VM that waits for a container (`depends_on`).
    VmAfterContainer,
    /// Extra names for a machine (`aliases`), dotted ones included.
    MachineAliases,
    /// Per-container process limits (`ulimits`).
    Ulimits,
    /// Link impairment (`tc`), applied on the path into a network.
    TrafficControl,
    /// Dual-stack networks (`ipv6`), with addresses derived from the octets.
    Ipv6,
    /// IPv6-only networks (`ipv6.only`).
    Ipv6Only,
    /// IPv6 addresses the target hands out (`ipv6.addresses: auto`).
    Ipv6Auto,
    /// Secrets delivered as files when the machines start (`machines.*.secrets`).
    Secrets,
    /// A secret file owned by a given user or group (`uid`, `gid`).
    SecretOwners,
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
    /// A network is routed by one of the environment's machines.
    Gateway {
        /// The network.
        network: String,
    },
    /// A shared volume is mounted by a container and by a VM.
    VolumeAcrossShapes {
        /// The volume.
        volume: String,
    },
    /// A VM waits for a container.
    VmDependsOnContainer {
        /// The VM.
        vm: String,
        /// The container it waits for.
        container: String,
    },
    /// A machine has `aliases`.
    MachineAliases,
    /// A machine sets `ulimits`.
    Ulimits {
        /// The first machine that does.
        machine: String,
    },
    /// A network has `tc`.
    TrafficControl,
    /// A network has an IPv6 block.
    Ipv6 {
        /// The first such network.
        network: String,
    },
    /// A network is IPv6-only.
    Ipv6Only {
        /// The first such network.
        network: String,
    },
    /// A network leaves its IPv6 addresses to the target.
    Ipv6Auto {
        /// The first such network.
        network: String,
    },
    /// A machine gets secrets.
    Secrets {
        /// The first machine that does.
        machine: String,
    },
    /// A machine's secret sets its owner (`uid` or `gid`).
    SecretOwner {
        /// The first machine whose secret does.
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
            Requirement::Gateway { .. } => Feature::Gateways,
            Requirement::VolumeAcrossShapes { .. } => Feature::VolumesAcrossShapes,
            Requirement::VmDependsOnContainer { .. } => Feature::VmAfterContainer,
            Requirement::MachineAliases => Feature::MachineAliases,
            Requirement::Ulimits { .. } => Feature::Ulimits,
            Requirement::TrafficControl => Feature::TrafficControl,
            Requirement::Ipv6 { .. } => Feature::Ipv6,
            Requirement::Ipv6Only { .. } => Feature::Ipv6Only,
            Requirement::Ipv6Auto { .. } => Feature::Ipv6Auto,
            Requirement::Secrets { .. } => Feature::Secrets,
            Requirement::SecretOwner { .. } => Feature::SecretOwners,
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
            Requirement::Gateway { .. } => f.write_str("networks with a `gateway` machine in hybrid environments come later"),
            Requirement::VolumeAcrossShapes { volume } => write!(
                f,
                "volume `{volume}` is shared by containers and VMs: one storage can't be both a Docker volume and an NFS export, for now"
            ),
            Requirement::VmDependsOnContainer { vm, container } => {
                write!(
                    f,
                    "VM `{vm}` depends on container `{container}`: VMs start first in a hybrid environment, for now"
                )
            }
            // Only Kubernetes lacks these today: the messages say so in its terms.
            Requirement::MachineAliases => f.write_str("machine `aliases` (dotted DNS names) aren't Kubernetes Service names"),
            Requirement::Ulimits { machine } => {
                write!(f, "machine `{machine}` sets `ulimits`: Kubernetes has no per-container process limits")
            }
            Requirement::TrafficControl => f.write_str("link impairment (`tc`) needs a router in the path; Kubernetes has none"),
            Requirement::Ipv6 { network } => write!(f, "network `{network}` is IPv6 (`ipv6`), which this target's networks don't carry yet"),
            Requirement::Ipv6Only { network } => write!(
                f,
                "network `{network}` is IPv6-only (`ipv6.only`): this target needs IPv4 on every network (its router, its set-up, its transport)"
            ),
            Requirement::Ipv6Auto { network } => write!(
                f,
                "network `{network}` leaves IPv6 addresses to the target (`ipv6.addresses: auto`), and nothing on this target hands them out: use `static`"
            ),
            Requirement::Secrets { machine } => write!(
                f,
                "machine `{machine}` gets `secrets`, which `isoloom run` delivers on the Docker and Kubernetes targets for now"
            ),
            Requirement::SecretOwner { machine } => write!(
                f,
                "a secret of machine `{machine}` sets `uid` or `gid`: a Kubernetes Secret volume's files belong to root"
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
    if spec.machines.values().any(|m| !m.aliases.is_empty()) {
        out.push(Requirement::MachineAliases);
    }
    out.extend(
        spec.networks
            .iter()
            .filter(|(_, n)| n.gateway.is_some())
            .map(|(name, _)| Requirement::Gateway { network: name.clone() }),
    );
    out.extend(limits(spec));
    out.extend(crate::ipv6::requirements(spec));
    out
}

/// What the containers' limits and the links' impairment ask for: at most one requirement of
/// each kind, the first machine or network that asks.
fn limits(spec: &Spec) -> Vec<Requirement> {
    let mut out = Vec::new();
    if let Some((name, _)) = spec.machines.iter().find(|(_, m)| !m.ulimits.is_empty()) {
        out.push(Requirement::Ulimits { machine: name.clone() });
    }
    if spec.networks.values().any(|n| n.tc.is_some()) {
        out.push(Requirement::TrafficControl);
    }
    out.extend(secrets(spec));
    out
}

/// What the machines' secrets ask for: delivering them at all, then owning their files.
fn secrets(spec: &Spec) -> Vec<Requirement> {
    let mut out = Vec::new();
    if let Some((name, _)) = spec.machines.iter().find(|(_, m)| !m.secrets.is_empty()) {
        out.push(Requirement::Secrets { machine: name.clone() });
    }
    let owned = |m: &isoloom_spec::Machine| m.secrets.iter().any(|s| s.mount().uid.is_some() || s.mount().gid.is_some());
    if let Some((name, _)) = spec.machines.iter().find(|(_, m)| owned(m)) {
        out.push(Requirement::SecretOwner { machine: name.clone() });
    }
    out
}

/// What mixing containers and VMs in one environment asks for: a shared volume mounted by both,
/// a VM waiting for a container. `shape` is each machine's shape on the target; only containers
/// and VMs count (an external or supplied machine is neither), so nothing comes out unless the
/// target runs both.
pub fn mixed_shapes(spec: &Spec, shape: impl Fn(&str) -> Option<MachineShape>) -> Vec<Requirement> {
    let is = |name: &str, s: MachineShape| shape(name) == Some(s);
    let volumes = spec.volumes.keys().filter(|v| {
        let users = spec.volume_users(v);
        users.iter().any(|u| is(u, MachineShape::Container)) && users.iter().any(|u| is(u, MachineShape::Vm))
    });
    let mut out: Vec<Requirement> = volumes.map(|v| Requirement::VolumeAcrossShapes { volume: v.clone() }).collect();
    for (name, m) in spec.machines.iter().filter(|(n, _)| is(n, MachineShape::Vm)) {
        if let Some(dep) = m.depends_on.iter().find(|d| is(d, MachineShape::Container)) {
            out.push(Requirement::VmDependsOnContainer {
                vm: name.clone(),
                container: dep.clone(),
            });
        }
    }
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
