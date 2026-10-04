//! The isoloom format, version 1: what an environment looks like from outside (machines,
//! networks, who reaches whom, the services that answer) and, per machine, how each kind of
//! target produces it (`docker:` and/or `vm:`). Background: Cyber CTF's
//! doc/architecture/LAB-RANGE-FORMAT.md.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// An environment spec. Field order in the file is kept (machines start in the order written).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub version: u32,
    /// Kebab-case id of the environment.
    pub name: String,
    pub networks: IndexMap<String, Network>,
    /// Allowed traffic between networks; everything else between networks is blocked.
    #[serde(default)]
    pub reach: Vec<Reach>,
    /// Values a runner may provide at launch (e.g. tokens). Never baked into images.
    #[serde(default)]
    pub inputs: Vec<String>,
    pub machines: IndexMap<String, Machine>,
    /// Black-box checks (scripts) run on the environment's networks; they prove the behavior on every target.
    #[serde(default)]
    pub checks: Vec<String>,
    /// Narrows the targets derived from the implementations (e.g. not tested on Proxmox yet).
    #[serde(default)]
    pub targets: Option<Vec<Target>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    /// An IPv4 block inside 10.0.0.0/8, e.g. 10.20.0.0/24.
    pub cidr: String,
    /// Whether machines on it may reach the internet (default true).
    #[serde(default = "yes")]
    pub internet: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reach {
    pub from: String,
    pub to: String,
    /// Empty = every port.
    #[serde(default)]
    pub ports: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    /// Network name -> last octet of the machine's address on it.
    pub networks: IndexMap<String, u8>,
    #[serde(default)]
    pub services: Vec<Service>,
    /// The spec inputs this machine receives (only these).
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub resources: Option<Resources>,
    /// Machines that must be ready (their services answering) before this one starts.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// The machine the user lands on. It may have no implementation: the runner supplies it.
    #[serde(default)]
    pub access: bool,
    /// How a container produces this machine.
    #[serde(default)]
    pub docker: Option<DockerImpl>,
    /// How a VM produces this machine (services installed natively, no Docker inside).
    #[serde(default)]
    pub vm: Option<VmImpl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub port: u16,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub http: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    #[serde(default)]
    pub cpus: Option<u32>,
    #[serde(default)]
    pub memory_mb: Option<u32>,
    #[serde(default)]
    pub disk_gb: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DockerImpl {
    /// A published image (exactly one of `image` and `build`).
    #[serde(default)]
    pub image: Option<String>,
    /// A build context in the project folder.
    #[serde(default)]
    pub build: Option<String>,
    /// One-shot jobs (scripts or folders in the project) run before the machine counts as ready.
    #[serde(default)]
    pub init: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmImpl {
    /// An OS name from [`KNOWN_OS`]; each target maps it to an image.
    pub os: String,
    /// Steps run inside the VM, in order (any tool: shell, Ansible...).
    #[serde(default)]
    pub provision: Vec<String>,
}

/// OS names a `vm:` may use. Each target maps them to its own images (e.g. Windows from an
/// evaluation ISO locally, a license-included image in the cloud).
pub const KNOWN_OS: &[&str] = &["debian-12", "ubuntu-24.04", "kali", "windows-server-2022", "windows-11"];

/// Where an environment can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    /// Containers on the player's Docker.
    Docker,
    /// A hosting service runs the Docker shape for the user (e.g. Cyber CTF hosted labs).
    Hosted,
    /// Docker on one cloud VM.
    CloudDocker,
    /// One local VM per machine (Vagrant).
    Vagrant,
    /// One VM per machine on the player's Proxmox.
    Proxmox,
    /// One cloud VM per machine.
    CloudVm,
    /// Export for players who run Ludus.
    Ludus,
}

impl Target {
    pub const ALL: [Target; 7] = [
        Target::Docker,
        Target::Hosted,
        Target::CloudDocker,
        Target::Vagrant,
        Target::Proxmox,
        Target::CloudVm,
        Target::Ludus,
    ];

    /// The implementation every machine needs for this target.
    pub fn needs(self) -> Shape {
        match self {
            Target::Docker | Target::Hosted | Target::CloudDocker => Shape::Docker,
            Target::Vagrant | Target::Proxmox | Target::CloudVm | Target::Ludus => Shape::Vm,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Target::Docker => "docker",
            Target::Hosted => "hosted",
            Target::CloudDocker => "cloud-docker",
            Target::Vagrant => "vagrant",
            Target::Proxmox => "proxmox",
            Target::CloudVm => "cloud-vm",
            Target::Ludus => "ludus",
        }
    }
}

/// The two ways a machine can be produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Docker,
    Vm,
}

impl Shape {
    pub fn key(self) -> &'static str {
        match self {
            Shape::Docker => "docker",
            Shape::Vm => "vm",
        }
    }
}
