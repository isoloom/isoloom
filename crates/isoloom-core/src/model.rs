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

/// A machine's CPU architecture. Every target has one: a container platform, a box
/// architecture, a cloud instance family. Defaults to x86-64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    /// x86-64 (amd64), the default.
    #[default]
    Amd64,
    /// 64-bit ARM (aarch64).
    Arm64,
}

impl Arch {
    /// The container platform value (`linux/amd64`, `linux/arm64`).
    pub fn docker_platform(self) -> &'static str {
        match self {
            Arch::Amd64 => "linux/amd64",
            Arch::Arm64 => "linux/arm64",
        }
    }

    /// The name Vagrant, Kubernetes and most tools use (`amd64`, `arm64`).
    pub fn id(self) -> &'static str {
        match self {
            Arch::Amd64 => "amd64",
            Arch::Arm64 => "arm64",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    /// Network name -> last octet of the machine's address on it.
    pub networks: IndexMap<String, u8>,
    /// The machine's CPU architecture (`amd64` or `arm64`). Defaults to `amd64`.
    #[serde(default)]
    pub arch: Arch,
    /// The machine needs kernel-level access (nested containers or VMs, loading kernel modules,
    /// raw devices). A container runs privileged; a VM already has it (its workload is root).
    #[serde(default)]
    pub privileged: bool,
    /// The machine's root filesystem is read-only (hardening): nothing can be written outside
    /// its declared `volumes`. Applies to containers; a VM's root stays writable.
    #[serde(default)]
    pub read_only: bool,
    /// Memory-backed (tmpfs) mount paths inside the machine, for scratch space that never
    /// touches disk (e.g. `/tmp`, `/run`). A VM sets these up in its own provisioning (fstab).
    #[serde(default)]
    pub tmpfs: Vec<String>,
    /// The size of `/dev/shm` (shared memory), e.g. `256m` or `1g`, for workloads that need more
    /// than the small default (browsers, some databases). A VM sizes it in its own provisioning.
    #[serde(default)]
    pub shm_size: Option<String>,
    /// How the machine resolves names: which DNS servers to use, search domains, and its own
    /// domain. Useful to point a Linux box at the lab's domain controller. A VM sets its resolver
    /// in its own provisioning (resolv.conf).
    #[serde(default)]
    pub dns: Option<Dns>,
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

/// How a Windows box answers WinRM. Most boxes (StefanScherer, gusztavvargadr) use plain HTTP
/// on 5985 with basic auth; some (GOAD's Windows Server 2025 box) only listen on HTTPS 5986.
/// Isoloom can't tell from the box name, so a non-default box declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Winrm {
    /// HTTP on 5985, basic auth (the default for the common Windows boxes).
    #[default]
    Plaintext,
    /// HTTPS on 5986, NTLM (certificate not verified).
    Ssl,
}

/// How a machine resolves names.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dns {
    /// DNS server addresses, in order (e.g. the lab's domain controller).
    #[serde(default)]
    pub servers: Vec<String>,
    /// Search domains appended to bare names.
    #[serde(default)]
    pub search: Vec<String>,
    /// The machine's own domain name.
    #[serde(default)]
    pub domain: Option<String>,
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
    /// How this Windows box answers WinRM, when it isn't the usual plain-HTTP box.
    #[serde(default)]
    pub winrm: Option<Winrm>,
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
    /// Containers and VMs together on the same networks (local VMs): each machine as a
    /// container when it can be one, else as a VM.
    Hybrid,
    /// One local VM per machine (Vagrant).
    Vagrant,
    /// One VM per machine on the player's Proxmox.
    Proxmox,
    /// One cloud VM per machine.
    CloudVm,
}

impl Target {
    pub const ALL: [Target; 9] = [
        Target::Docker,
        Target::Hosted,
        Target::DockerVm,
        Target::CloudDocker,
        Target::Kubernetes,
        Target::Hybrid,
        Target::Vagrant,
        Target::Proxmox,
        Target::CloudVm,
    ];

    /// The implementation every machine needs for this target.
    pub fn needs(self) -> Shape {
        match self {
            Target::Docker | Target::Hosted | Target::DockerVm | Target::CloudDocker | Target::Kubernetes => Shape::Docker,
            Target::Vagrant | Target::Proxmox | Target::CloudVm => Shape::Vm,
            Target::Hybrid => Shape::Either,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Target::Docker => "docker",
            Target::Hosted => "hosted",
            Target::DockerVm => "docker-vm",
            Target::CloudDocker => "cloud-docker",
            Target::Kubernetes => "kubernetes",
            Target::Hybrid => "hybrid",
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
    /// Either implementation, machine by machine.
    Either,
}

impl Shape {
    pub fn key(self) -> &'static str {
        match self {
            Shape::Docker => "docker",
            Shape::Vm => "vm",
            Shape::Either => "docker` or `vm",
        }
    }
}
