//! The isoloom format, version 1: what an environment looks like from outside (machines,
//! networks, who reaches whom, the services that answer) and, per machine, how each kind of
//! target produces it (`docker:` and/or `vm:`).

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An environment spec. Field order in the file is kept (machines start in the order written).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// Provisioning across machines, once every machine is up: Ansible playbooks run from a
    /// controller on the environment's networks, with an inventory Isoloom writes.
    #[serde(default)]
    pub provision: Vec<Provision>,
    /// Black-box checks (scripts) run on the environment's networks; they prove the behavior on every target.
    #[serde(default)]
    pub checks: Vec<String>,
    /// Narrows the targets derived from the implementations (e.g. not tested on Proxmox yet).
    #[serde(default)]
    pub targets: Option<Vec<Target>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Network {
    /// An IPv4 block inside 10.0.0.0/8, e.g. 10.20.0.0/24.
    pub cidr: String,
    /// Whether machines on it may reach the internet (default true).
    #[serde(default = "yes")]
    pub internet: bool,
    /// A machine of the environment that routes this network (an edge firewall): it takes the
    /// gateway address, and its own configuration decides what crosses it. Isoloom adds no
    /// router of its own here, so `reach` and `internet` on this network become expected
    /// behavior for the checks to verify.
    #[serde(default)]
    pub gateway: Option<String>,
    /// How the Docker target lays this network out, when it differs from the other targets.
    #[serde(default)]
    pub docker: Option<NetworkDocker>,
}

/// A network on the Docker target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkDocker {
    /// The block to use on Docker instead of `cidr` (same size, inside 10.0.0.0/8). Without
    /// it, a network outside 10.0.0.0/8 is moved into 10.0.0.0/8 automatically, since Docker's
    /// own pools, Docker Desktop and home networks use 172.16.0.0/12 and 192.168.0.0/16.
    pub cidr: String,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reach {
    pub from: String,
    pub to: String,
    /// Empty = every port.
    #[serde(default)]
    pub ports: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
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
    /// Data that survives restarts and re-creation of the machine, until the environment is
    /// destroyed: a name -> an absolute path in the machine.
    #[serde(default)]
    pub volumes: IndexMap<String, String>,
    /// The machine the user lands on. It may have no implementation: the runner supplies it.
    #[serde(default)]
    pub access: bool,
    /// How a container produces this machine.
    #[serde(default)]
    pub docker: Option<DockerImpl>,
    /// How a VM produces this machine (services installed natively, no Docker inside).
    #[serde(default)]
    pub vm: Option<VmImpl>,
    /// Filled by the runner's image table (an access machine the spec leaves to the runner):
    /// a stock image, kept running idle for the user to work from. Never in a spec file.
    #[serde(skip)]
    #[schemars(skip)]
    pub supplied: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub port: u16,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub http: bool,
    /// Reachable from the user's machine on this port (loopback only), besides the
    /// environment's own networks.
    #[serde(default)]
    pub publish: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    #[serde(default)]
    pub cpus: Option<u32>,
    #[serde(default)]
    pub memory_mb: Option<u32>,
    #[serde(default)]
    pub disk_gb: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VmImpl {
    /// An OS name from [`KNOWN_OS`]; each target maps it to an image.
    pub os: String,
    /// Steps run inside the VM, in order: `.sh` on Linux (and Ansible playbooks, `.yml`),
    /// `.ps1` on Windows.
    #[serde(default)]
    pub provision: Vec<String>,
    /// The image to use instead of the built-in one for `os`, per target.
    #[serde(default)]
    pub image: Option<VmImage>,
}

/// A machine's own image, per target, instead of the built-in one for its OS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VmImage {
    /// A Vagrant box name (e.g. `StefanScherer/windows_2019`).
    #[serde(default)]
    pub vagrant: Option<String>,
    /// The box version to pin.
    #[serde(default)]
    pub vagrant_version: Option<String>,
}

/// One environment-level provisioning step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provision {
    /// An Ansible playbook in the project, run against the whole environment.
    pub ansible: String,
    /// More inventory files in the project (groups, variables), next to the one Isoloom writes
    /// (every machine's address and connection, in the `linux` or `windows` group).
    #[serde(default)]
    pub inventory: Vec<String>,
    /// Inventory groups: a name -> the machines in it.
    #[serde(default)]
    pub groups: IndexMap<String, Vec<String>>,
    /// Extra variables for the playbook.
    #[serde(default)]
    pub vars: IndexMap<String, String>,
    /// The Ansible Galaxy requirements to install first (collections, roles); by default
    /// `requirements.yml` next to the playbook, when there is one.
    #[serde(default)]
    pub requirements: Option<String>,
}

/// OS names a `vm:` may use. Each target maps them to its own images (e.g. Windows from an
/// evaluation ISO locally, a license-included image in the cloud).
pub const KNOWN_OS: &[&str] = &[
    "debian-11",
    "debian-12",
    "debian-13",
    "ubuntu-20.04",
    "ubuntu-22.04",
    "ubuntu-24.04",
    "rocky-9",
    "almalinux-9",
    "centos-7",
    "fedora-42",
    "kali",
    "windows-10",
    "windows-11",
    "windows-server-2016",
    "windows-server-2019",
    "windows-server-2022",
    "windows-server-2025",
];

/// Where an environment can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    /// Containers on the player's Docker.
    Docker,
    /// A hosting service runs the Docker shape for the user.
    Hosted,
    /// Docker on one VM: on this machine, an ESXi host or a Proxmox server.
    DockerVm,
    /// Docker on one cloud VM.
    CloudDocker,
    /// The containers on a Kubernetes cluster.
    Kubernetes,
    /// One local VM per machine (Vagrant).
    Vagrant,
    /// One VM per machine on the player's Proxmox.
    Proxmox,
    /// One cloud VM per machine.
    CloudVm,
}

impl Target {
    pub const ALL: [Target; 8] = [
        Target::Docker,
        Target::Hosted,
        Target::DockerVm,
        Target::CloudDocker,
        Target::Kubernetes,
        Target::Vagrant,
        Target::Proxmox,
        Target::CloudVm,
    ];

    /// The implementation every machine needs for this target.
    pub fn needs(self) -> Shape {
        match self {
            Target::Docker | Target::Hosted | Target::DockerVm | Target::CloudDocker | Target::Kubernetes => Shape::Docker,
            Target::Vagrant | Target::Proxmox | Target::CloudVm => Shape::Vm,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Target::Docker => "docker",
            Target::Hosted => "hosted",
            Target::DockerVm => "docker-vm",
            Target::CloudDocker => "cloud-docker",
            Target::Kubernetes => "kubernetes",
            Target::Vagrant => "vagrant",
            Target::Proxmox => "proxmox",
            Target::CloudVm => "cloud-vm",
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
