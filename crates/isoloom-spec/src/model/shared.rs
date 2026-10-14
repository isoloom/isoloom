//! What every machine or several machines share: `common:`, groups, tools, external
//! resources, shared volumes and ulimits.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Arch, Dns, Mount, Resources, ScalarSchema, Tmpfs, Virtiofs, VmImage};
#[cfg(doc)]
use super::{Machine, Spec};

/// Fields several machines share, through `common:` or a group: everything about how a machine
/// is built, nothing about what it is (its networks, services and `access` stay its own).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Shared {
    /// The machine's CPU architecture, as `arch` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<Arch>,
    /// Kernel-level access, as `privileged` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privileged: Option<bool>,
    /// A read-only root filesystem, as `read_only` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    /// Memory-backed mount paths, as `tmpfs` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmpfs: Option<Vec<Tmpfs>>,
    /// The size of `/dev/shm`, as `shm_size` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shm_size: Option<String>,
    /// Name resolution, as `dns` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns: Option<Dns>,
    /// Environment variables, as `environment` on a machine (merged key by key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<IndexMap<String, ScalarSchema>>")]
    pub environment: Option<IndexMap<String, serde_json::Value>>,
    /// Kernel parameters, as `sysctls` on a machine (merged key by key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<IndexMap<String, ScalarSchema>>")]
    pub sysctls: Option<IndexMap<String, serde_json::Value>>,
    /// Process limits, as `ulimits` on a machine (merged key by key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ulimits: Option<IndexMap<String, Ulimit>>,
    /// The spec inputs the machines receive, as `inputs` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Vec<String>>,
    /// CPUs, memory and disk, as `resources` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<Resources>,
    /// Machines that must be ready first, as `depends_on` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
    /// Data that survives restarts, as `volumes` on a machine (merged key by key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volumes: Option<IndexMap<String, Mount>>,
    /// Completes the `docker:` of machines that have one (image, build, init, idle).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker: Option<SharedDocker>,
    /// Completes the `vm:` of machines that have one (os, provision, image).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vm: Option<SharedVm>,
}

/// A shared `docker:` part: each field optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SharedDocker {
    /// A published image, as `docker.image` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// A build context in the project folder, as `docker.build` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    /// One-shot jobs run before the machine counts as ready, as `docker.init` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub init: Option<Vec<String>>,
    /// Keep a container without a service of its own running, as `docker.idle` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle: Option<bool>,
}

/// A shared `vm:` part: each field optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SharedVm {
    /// An OS name, as `vm.os` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// Steps run inside the VM, as `vm.provision` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provision: Option<Vec<String>>,
    /// Images per target instead of the OS name's, as `vm.image` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<VmImage>,
    /// A virtual TPM 2.0, as `vm.tpm` on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpm: Option<bool>,
}

/// A group: the machines in it, and the fields they share.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Group {
    /// Machine names, globs (`ws*`), or other groups.
    pub members: Vec<String>,
    /// The fields the members share (a member's own value wins).
    #[serde(flatten)]
    pub shared: Shared,
}

/// A tool attached beside the environment. The built-in recipe `shell` (an empty block) is a
/// toolbox on every network: tcpdump, nmap, curl, dig, netcat (a netshoot container on Docker, a
/// Debian VM on local VMs). Any other name takes a container image (container targets).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    /// A container image (needed unless the name is a built-in recipe).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// The command to run, when the image's own isn't right.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    /// A port the tool serves (its web UI), reachable from every network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// The port on the host's loopback (`127.0.0.1`) that forwards to the tool's `port`, so a
    /// browser on the host can open its web UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish: Option<u16>,
}

/// Built-in tool recipes.
pub const TOOL_RECIPES: &[&str] = &["shell"];

/// An existing machine's SSH endpoint, for the `external` target.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct External {
    /// An address or hostname reachable from where Isoloom runs.
    pub address: String,
    /// The SSH user (default `root`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// The SSH port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// A private key file (default: the SSH agent and config).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// A volume of the environment, mounted by one machine or several (see [`Spec::volumes`]): one
/// named volume on Docker, one claim on Kubernetes (its users on its owner's node), and on VMs
/// the owner's directory served over NFS to the others, mounted at their own paths. Declared
/// here, a volume can also outlive the environment (`persistent`) or be one that exists already
/// (`external`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SharedVolume {
    /// The machine that holds the data (serves it over NFS on the VM targets). Default: the
    /// first machine that mounts it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Keep its data when the environment is torn down: `isoloom down` leaves it, and the next
    /// `run` finds it again; `isoloom down --volumes` removes it. Docker and Kubernetes; a VM's
    /// storage goes with the VM.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub persistent: bool,
    /// An existing volume, by its name on the platform (a Docker volume, a Kubernetes claim in
    /// the environment's namespace): Isoloom mounts it and never creates or removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<String>,
    /// On Proxmox, the volume as a folder of the server, shared into each VM that mounts it
    /// (virtiofs) instead of over NFS from its owner: `virtiofs: {}` takes the defaults. Every
    /// other target ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub virtiofs: Option<Virtiofs>,
}

/// A process limit (see [`Machine::ulimits`]): one value for the soft and hard limits, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Ulimit {
    /// One value for both the soft and the hard limit.
    Both(u64),
    /// The soft limit (what a process gets) and the hard one (how far it may raise it).
    Pair {
        /// The limit a process starts with.
        soft: u64,
        /// The ceiling a process may raise its soft limit to.
        hard: u64,
    },
}

impl Ulimit {
    /// The limit a process starts with.
    pub fn soft(self) -> u64 {
        match self {
            Ulimit::Both(n) | Ulimit::Pair { soft: n, .. } => n,
        }
    }

    /// The ceiling a process may raise its soft limit to.
    pub fn hard(self) -> u64 {
        match self {
            Ulimit::Both(n) | Ulimit::Pair { hard: n, .. } => n,
        }
    }
}

/// The process limits Linux knows (`ulimit`, `limits.conf`, systemd's `Limit*`).
pub const ULIMITS: &[&str] = &[
    "core",
    "cpu",
    "data",
    "fsize",
    "locks",
    "memlock",
    "msgqueue",
    "nice",
    "nofile",
    "nproc",
    "rss",
    "rtprio",
    "rttime",
    "sigpending",
    "stack",
];
