//! A container machine's objects: its Deployment, its volumes' claims, its Services (its name
//! for the others, and what it publishes), and the policies of what it publishes and whether
//! it is offline.

use std::fmt::Write;

use isoloom_plan::environment::Build;
use isoloom_plan::{Environment, MachinePlan, MachineShape, PublishedPort, VolumeKind};
use isoloom_spec::model::{self, Protocol};
use isoloom_spec::shell;

use crate::network::{self, MACHINE_LABEL};
use crate::objects::{
    DeploymentSpec, Image, Labels, Manifest, Metadata, NetworkPolicySpec, Peer, PodTemplate, PolicyPort, Rule, Selector, ServicePort, ServiceSpec, Strategy,
};
use crate::pod;

/// One container machine and what its objects are built from.
pub(crate) struct Machine<'a> {
    /// The environment it is part of.
    pub env: &'a Environment,
    /// Its name.
    pub name: &'a str,
    /// Its plan.
    pub plan: &'a MachinePlan,
    /// What the plan doesn't model yet: privileges, services, `init:` jobs, tmpfs, DNS.
    pub spec: &'a model::Machine,
    /// The reference its image is pulled as.
    pub reference: &'a str,
    /// How the project builds its image, if it does.
    pub build: Option<&'a Build>,
}

impl<'a> Machine<'a> {
    /// The environment's container machines, in spec order.
    pub fn all(env: &'a Environment) -> Vec<Machine<'a>> {
        env.machines
            .iter()
            .filter(|(_, mp)| mp.shape == MachineShape::Container)
            .filter_map(|(name, plan)| {
                let Some(isoloom_plan::Image::Container { reference, build }) = &plan.image else {
                    return None;
                };
                Some(Machine {
                    env,
                    name,
                    plan,
                    spec: env.spec().machines.get(name)?,
                    reference,
                    build: build.as_ref(),
                })
            })
            .collect()
    }

    /// Its image, a value of its own in the chart.
    pub fn image(&self) -> Image {
        Image::Machine {
            machine: self.name.to_string(),
            reference: self.reference.to_string(),
        }
    }

    /// Whether it has a port to probe or a healthcheck of its own (its readiness, what its
    /// `init:` jobs wait for).
    pub fn probed(&self) -> bool {
        self.spec.has_readiness()
    }

    /// Its `init:` scripts, in order.
    pub fn init(&self) -> &'a [String] {
        self.spec.docker.as_ref().map_or(&[], |d| d.init.as_slice())
    }

    /// The selector of its pod.
    pub fn selector(&self) -> Labels {
        Labels::from([(MACHINE_LABEL.to_string(), self.name.to_string())])
    }

    /// Whether it holds volume `v`'s claim: its own volume, or a shared one it owns.
    pub fn holds(&self, v: &VolumeKind) -> bool {
        match v {
            VolumeKind::Own => true,
            VolumeKind::Shared { owner, .. } => owner == self.name,
        }
    }

    /// How the project builds its image, as the comment of the manifests says to run it.
    pub fn build_command(&self) -> Option<String> {
        let b = self.build?;
        let mut cmd = format!("docker build -t {}", self.reference);
        if let Some(f) = &b.dockerfile {
            let _ = write!(cmd, " -f {f}");
        }
        if let Some(t) = &b.target {
            let _ = write!(cmd, " --target {t}");
        }
        for (k, v) in &b.args {
            let _ = write!(cmd, " --build-arg {}", shell::quote(&format!("{k}={v}")));
        }
        Some(format!("{cmd} {}", b.context))
    }
}

/// Every object of machine `m`, in order: its Deployment, its claims, its Services and its
/// policies.
pub(crate) fn objects(m: &Machine) -> Vec<Manifest> {
    let mut out = vec![deployment(m)];
    out.extend(crate::storage::claims(m));
    out.extend(crate::files::config_map(m));
    out.extend(service(m));
    out.extend(published(m));
    if m.plan.offline {
        out.push(network::offline(&format!("offline-{}", m.name), Selector::of(MACHINE_LABEL, m.name)));
    }
    out
}

/// The machine itself: one pod, recreated (its claims are ReadWriteOnce).
fn deployment(m: &Machine) -> Manifest {
    let spec = DeploymentSpec {
        replicas: 1,
        strategy: Strategy { kind: "Recreate" },
        selector: Selector { match_labels: m.selector() },
        template: PodTemplate {
            metadata: Metadata::labelled(network::pod_labels(Some(m.name), m.plan.addresses.keys())),
            spec: pod::spec(m),
        },
    };
    Manifest::deployment(m.name, spec)
}

/// `UDP` for a UDP port; TCP, the default, left out.
pub(crate) fn udp(p: Protocol) -> Option<&'static str> {
    (!p.is_tcp()).then_some("UDP")
}

/// A Service port's name, unique within the Service: `p53`, and `p53-udp` for UDP (a port
/// served over both protocols is two entries).
fn port_name(port: u16, p: Protocol) -> String {
    if p.is_tcp() { format!("p{port}") } else { format!("p{port}-udp") }
}

/// Its name, for the other machines: a Service on every port it serves.
fn service(m: &Machine) -> Option<Manifest> {
    if m.spec.ports.is_empty() {
        return None;
    }
    let ports = m
        .spec
        .ports
        .iter()
        .map(|sv| ServicePort {
            name: port_name(sv.port, sv.protocol),
            port: sv.port,
            target_port: sv.port,
            protocol: udp(sv.protocol),
        })
        .collect();
    let (ip_family_policy, ip_families) = crate::ipv6::families(m.env, m.plan);
    let spec = ServiceSpec {
        kind: None,
        ip_family_policy,
        ip_families,
        selector: m.selector(),
        ports,
    };
    Some(Manifest::service(m.name, spec))
}

/// What it publishes: reachable from outside the cluster, a LoadBalancer Service on the
/// published ports, and the policy letting anything in on them.
fn published(m: &Machine) -> Vec<Manifest> {
    let ports: &[PublishedPort] = &m.plan.ports;
    if ports.is_empty() {
        return Vec::new();
    }
    let (ip_family_policy, ip_families) = crate::ipv6::families(m.env, m.plan);
    let service = ServiceSpec {
        kind: Some("LoadBalancer"),
        ip_family_policy,
        ip_families,
        selector: m.selector(),
        ports: ports
            .iter()
            .map(|p| ServicePort {
                name: port_name(p.publish, p.protocol),
                port: p.publish,
                target_port: p.port,
                protocol: udp(p.protocol),
            })
            .collect(),
    };
    let anyone = Rule {
        from: crate::ipv6::anyone(m.env, m.plan)
            .into_iter()
            .map(|b| Peer {
                ip_block: Some(b),
                ..Peer::default()
            })
            .collect(),
        ports: ports
            .iter()
            .map(|p| PolicyPort {
                port: p.port,
                protocol: udp(p.protocol),
            })
            .collect(),
        ..Rule::default()
    };
    let policy = NetworkPolicySpec {
        pod_selector: Selector { match_labels: m.selector() },
        ingress: vec![anyone],
        ..NetworkPolicySpec::default()
    };
    vec![
        Manifest::service(&format!("{}-published", m.name), service),
        Manifest::network_policy(&format!("publish-{}", m.name), policy),
    ]
}
