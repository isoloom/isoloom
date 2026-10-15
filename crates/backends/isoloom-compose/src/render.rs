//! The Compose file of a planned [`Environment`]: a service per container machine (with its
//! `init:` jobs and network sidecar), Isoloom's router, VLAN switches, probes, check runners and
//! tools, then the networks and volumes; every resource labelled as the environment's.

pub mod files;
mod hardware;
mod machine;
mod process;
mod secrets;

pub(crate) use machine::mac;

use std::fmt::Write as _;

use indexmap::IndexMap;
use isoloom_manifest::Manifest;
use isoloom_plan::{Environment, Feature, MachineShape};
use isoloom_render::backend::{self, Backend, Files, RenderError};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header, trunks};
use isoloom_spec::model::{Arch, Target};

use crate::appliances;
use crate::docker::{DIR, ENVIRONMENT_LABEL, MANAGED_LABEL, PROBE_IMAGE, TOOLBOX_IMAGE, probe_service, tool_service};
use crate::file::{Attachment, ComposeFile, Ipam, Network, Service, Subnet, Volume};
use crate::sidecars;
use crate::storage;
use machine::machine;

/// What Compose can render: network appliances (wired the way Docker attaches networks), aliases,
/// gateway machines, ulimits, `tc` (in a sidecar), IPv6 networks of every kind, secrets
/// (Compose's own, owners included), project files with their owner and mode (copied in from
/// Compose's environment), existing Docker networks, and the host's GPUs and devices
/// (the containers run on the user's own Docker host, which has them). No controller: no
/// `provision:`, no Ansible checks; `exec` checks run inside a container. No bridged
/// networks: a container reaches the host's LAN through a macvlan network made beforehand
/// (`external`). Data disks are named volumes, a tmpfs takes a mode, and a volume can be kept
/// past `down` or exist already; no raw disk (a container gets no block device).
pub fn capabilities() -> isoloom_plan::Capabilities {
    isoloom_plan::Capabilities::of([
        Feature::NetworkAppliances,
        Feature::Gateways,
        Feature::MachineAliases,
        Feature::Ulimits,
        Feature::TrafficControl,
        Feature::Ipv6,
        Feature::Ipv6Only,
        Feature::Ipv6Auto,
        Feature::Secrets,
        Feature::SecretOwners,
        Feature::OwnedProjectFiles,
        Feature::ExternalNetworks,
        Feature::MachineProcess,
        Feature::ContainerGpus,
        Feature::ContainerDevices,
        // Docker's DNS answers the names under the private zone's domain (network aliases).
        Feature::PrivateDns,
        Feature::DataDisks,
        Feature::TmpfsModes,
        Feature::PersistentVolumes,
        Feature::ExternalVolumes,
    ])
}

/// What a Compose file run on a VM can render (`docker-vm`, `cloud-docker`): Compose's, but
/// existing networks, which a fresh Docker host VM has none of, and the host's GPUs and
/// devices, which a VM Isoloom creates doesn't have, and volumes kept past `down` or existing
/// already (the storage goes with the VM). Secrets reach the VM over its own
/// connection (`isoloom run`), and Compose takes them from there.
pub fn capabilities_on_a_vm() -> isoloom_plan::Capabilities {
    isoloom_plan::Capabilities::of([
        Feature::NetworkAppliances,
        Feature::Gateways,
        Feature::MachineAliases,
        Feature::Ulimits,
        Feature::TrafficControl,
        Feature::DataDisks,
        Feature::TmpfsModes,
        Feature::Ipv6,
        Feature::Ipv6Only,
        Feature::Ipv6Auto,
        Feature::Secrets,
        Feature::SecretOwners,
        Feature::OwnedProjectFiles,
        Feature::MachineProcess,
        Feature::PrivateDns,
    ])
}

/// The Compose backend of the `docker` and `hosted` targets (a hosting service runs the same
/// Compose file).
#[derive(Debug, Clone, Copy)]
pub struct ComposeBackend(pub Target);

impl Backend for ComposeBackend {
    fn target(&self) -> Target {
        self.0
    }

    fn capabilities(&self) -> isoloom_plan::Capabilities {
        capabilities()
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        render(self.0, env)
    }

    /// The `docker` target's manifest; a hosting service runs the file its own way.
    fn run_manifest(&self, env: &Environment) -> Result<Option<Manifest>, RenderError> {
        if self.0 != Target::Docker {
            return Ok(None);
        }
        crate::manifest::docker(env, &compose_file(self.0, env)?.file).map(Some)
    }
}

/// The Compose file of `env` and the files it mounts (check runners, appliance configurations),
/// refused as `target` when `env` needs what Compose can't do.
pub fn render(target: Target, env: &Environment) -> Result<Files, RenderError> {
    let Rendered { file, files: mounted } = compose_file(target, env)?;
    let yaml = to_yaml(target, &file)?;
    let mut files = vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/compose.yml"),
        contents: format!("{}{USAGE}{}{}{}\n{yaml}", header("#"), secrets::usage(&file), files::usage(env), moved(env)),
    }];
    files.extend(mounted);
    Ok(files)
}

/// A Compose file before it is written, and the files it mounts.
#[derive(Debug, Clone)]
pub struct Rendered {
    /// The Compose file.
    pub file: ComposeFile,
    /// The files next to it: check runners, appliance configurations, Dockerfiles.
    pub files: Files,
}

/// The typed Compose file of `env` and the files it mounts, for a target that adapts it before
/// writing it (`hybrid` plugs it into its Docker host's networks). Refused as `target` when
/// `env` needs what Compose can't do.
pub fn compose_file(target: Target, env: &Environment) -> Result<Rendered, RenderError> {
    backend::check(target, env, &capabilities())?;
    let mut out = Out::default();
    let ts = trunks::trunks(env.spec());
    sidecars::router_and_switches(env, &ts, &mut out.services);
    for (name, mp) in &env.machines {
        if mp.shape == MachineShape::Container {
            machine(env, name, mp, &ts, &mut out);
        }
    }
    storage::folders(env, &mut out.services);
    let runners = sidecars::check_runners(env, &mut out.services);
    tools(env, &mut out.services);
    let mut file = ComposeFile {
        name: env.name.clone(),
        networks: networks(env, &ts),
        volumes: volumes(env, &out.probe_archs),
        secrets: secrets::of_file(env),
        configs: files::configs(env),
        ..ComposeFile::default()
    };
    probes(env, &out.probe_archs, &mut out.services);
    file.services = out.services;
    label(env, &mut file);
    // Existing networks aren't the environment's: unlabelled, so nothing collects them.
    file.networks.extend(crate::outside::networks(env));
    let mut files = runners;
    files.extend(out.files);
    Ok(Rendered { file, files })
}

/// `file` as YAML, a failure reported as `target`'s.
pub fn to_yaml(target: Target, file: &ComposeFile) -> Result<String, RenderError> {
    serde_yaml_ng::to_string(file).map_err(|e| RenderError::Generate(isoloom_render::GenerateError::Unsupported { target, what: e.to_string() }))
}

const USAGE: &str = "# Start:  docker compose -f .isoloom/docker/compose.yml up -d --wait\n# Checks: isoloom test docker (or: docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check, and isoloom-check-<machine>)\n# Stop:   docker compose -f .isoloom/docker/compose.yml down -v\n";

/// What rendering the machines collects besides their services.
#[derive(Default)]
struct Out {
    services: IndexMap<String, Service>,
    /// Appliance configurations and Dockerfiles, next to the Compose file.
    files: Vec<GeneratedFile>,
    /// The architectures of machines that need the probe: one copy job (and volume) each.
    probe_archs: Vec<Arch>,
}

/// The comment saying which networks moved to their Docker block.
fn moved(env: &Environment) -> String {
    env.networks
        .iter()
        .filter(|(_, n)| n.declared_cidr != n.cidr)
        .fold(String::new(), |mut out, (name, n)| {
            let _ = writeln!(
                out,
                "# On Docker, network `{name}` uses {} instead of {} (same last octets).",
                n.cidr, n.declared_cidr
            );
            out
        })
}

/// Tools: observers on every network at the reserved addresses, outside the environment's
/// contract (no reach rule, no check names them).
fn tools(env: &Environment, services: &mut IndexMap<String, Service>) {
    let spec = env.spec();
    for (i, (name, tool)) in spec.tools.iter().enumerate() {
        let shell = name == "shell";
        let mut t = Service {
            image: Some(env.images.pull(tool.image.as_deref().unwrap_or(TOOLBOX_IMAGE))),
            hostname: Some(name.clone()),
            command: tool.command.clone(),
            restart: Some("unless-stopped".into()),
            ..Service::default()
        };
        if tool.command.is_empty() && shell {
            t.entrypoint = vec!["sleep".into(), "infinity".into()];
        }
        if shell {
            t.cap_add = vec!["NET_ADMIN".into(), "NET_RAW".into()];
        }
        // The spec is validated: every block parses.
        for (net, n) in &spec.networks {
            // On an IPv6-only network, Docker hands the tool its address.
            if crate::ipv6::only(env, net) {
                t.networks.insert(net.clone(), None);
            } else if let Some(cidr) = isoloom_spec::validate::Cidr::parse(&n.cidr) {
                t.networks.insert(net.clone(), Attachment::at(cidr.tool(i)));
            }
        }
        if let (Some(port), Some(host)) = (tool.port, tool.publish) {
            t.ports = vec![format!("${{ISOLOOM_PUBLISH_ADDRESS:-127.0.0.1}}:{host}:{port}")];
        }
        services.insert(tool_service(name), t);
    }
}

/// The one-shot services that copy the probe, one per architecture.
fn probes(env: &Environment, archs: &[Arch], services: &mut IndexMap<String, Service>) {
    for arch in archs {
        let name = probe_service(*arch);
        let p = Service {
            image: Some(env.images.pull(PROBE_IMAGE)),
            platform: Some(arch.docker_platform().to_string()),
            entrypoint: vec!["/bin/cp".into(), "/bin/busybox".into(), "/probe/busybox".into()],
            volumes: vec![format!("{name}:/probe").into()],
            network_mode: Some("none".into()),
            restart: Some("no".into()),
            ..Service::default()
        };
        services.insert(name, p);
    }
}

/// The networks: each on its block, Docker at the host's address. Without routing,
/// `internet: false` is Docker's internal network; then the trunks' links and the appliances'
/// management network.
fn networks(env: &Environment, ts: &[trunks::Trunk]) -> IndexMap<String, Network> {
    let mut out: IndexMap<String, Network> = env
        .networks
        .iter()
        .map(|(name, n)| {
            let subnet = Subnet {
                subnet: n.cidr.clone(),
                gateway: Some(n.host.to_string()),
                ip_range: None,
            };
            (name.clone(), crate::ipv6::network(n, subnet, sidecars::internal(env, name)))
        })
        .collect();
    for t in ts {
        out.insert(
            trunks::link_network(t),
            Network {
                internal: true,
                ..Network::default()
            },
        );
    }
    if !appliances::appliances(env.spec()).is_empty() {
        let subnet = Subnet {
            subnet: appliances::MGMT_CIDR.into(),
            gateway: None,
            ip_range: None,
        };
        out.insert(
            appliances::MGMT_NETWORK.into(),
            Network {
                ipam: Some(Ipam { config: vec![subnet] }),
                ..Network::default()
            },
        );
    }
    out
}

/// The named volumes: the machines' (and their disks'), then the probes'.
fn volumes(env: &Environment, archs: &[Arch]) -> IndexMap<String, Volume> {
    let mut out = storage::declared(env);
    out.extend(archs.iter().map(|a| (probe_service(*a), Volume::default())));
    out
}

/// Everything the environment creates says so (containers, built images, networks, volumes),
/// so leftovers can be told apart and collected (`isoloom gc`) without guessing.
fn label(env: &Environment, file: &mut ComposeFile) {
    let ours = [
        (MANAGED_LABEL.to_string(), "true".to_string()),
        (ENVIRONMENT_LABEL.to_string(), env.name.clone()),
    ];
    for svc in file.services.values_mut() {
        svc.labels.extend(ours.clone());
        if let Some(b) = &mut svc.build {
            b.labels.extend(ours.clone());
        }
    }
    for n in file.networks.values_mut() {
        n.labels.extend(ours.clone());
    }
    // An existing volume isn't the environment's: Compose takes only its name.
    for v in file.volumes.values_mut().filter(|v| !v.external) {
        v.labels.extend(ours.clone());
    }
}
