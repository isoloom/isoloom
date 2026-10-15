//! A machine as seen from outside: its OS, ports, architecture, resources and the settings it
//! carries for every target.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{CommandLine, Disk, Dns, DockerImpl, External, Healthcheck, MachineFile, MachineSecret, Mount, Tmpfs, Ulimit, VmImpl};

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

/// A machine of the environment: where it sits on the networks, what it serves, and how each
/// kind of target produces it (`docker:`, `vm:`, `external:`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)] // the spec's own keys: each is a switch a user writes.
pub struct Machine {
    /// Network name -> last octet of the machine's address on it. Every machine has one, but a
    /// LAN's `switch`, whose ports the LAN gives it. A list (`lan: [10, 11, 12]`) gives the
    /// machine several addresses on the network: the first is its primary one, the one its name
    /// resolves to, and the others are on the same interface.
    #[serde(default)]
    #[schemars(with = "IndexMap<String, super::Octets>")]
    pub networks: IndexMap<String, u8>,
    /// The addresses after the first of each network written as a list, split from `networks`
    /// when the spec is parsed ([`crate::addresses`]): not a field a spec writes.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    #[schemars(skip)]
    pub more_addresses: IndexMap<String, Vec<u8>>,
    /// How many of this machine (2 to 99): clones named `<name>-01`, `<name>-02`, ..., each one
    /// address further along on every network. Where the spec names this machine, it means
    /// every clone. Expanded before anything else reads the spec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u8>,
    /// More DNS names the machine answers to besides its own (a fully qualified name an
    /// application has baked in, like `api.example.com`), on every network it is on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// The machine's OS as `<family>@<version>` (`debian@12`, `windows-server@2022`), or the
    /// family alone for its pinned default version (`debian` is `debian@12`). `vm.os` defaults
    /// to it, and a machine with neither `docker:` nor `vm:` runs as a VM of this OS
    /// (`vm: { os }`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// The machine's CPU architecture (`amd64` or `arm64`). Defaults to `amd64`.
    #[serde(default)]
    pub arch: Arch,
    /// A bandwidth cap on each of the machine's network links, in both directions, as `tc`
    /// writes a rate (`10mbit`, `512kbit`): a machine on a slow line, while `networks.*.tc`
    /// slows a whole network down on its router.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<String>,
    /// The machine needs kernel-level access (nested containers, loading kernel modules, raw
    /// devices). A container runs privileged; a VM already has it (its workload is root). VMs
    /// inside a VM need `nested_virtualization`.
    #[serde(default)]
    pub privileged: bool,
    /// The machine runs VMs of its own: its VM gets the CPU's virtualization extensions (each
    /// hypervisor's nested virtualization). A VM only: a container can't, so the machine needs
    /// a `vm:`. Targets whose VMs can't nest refuse it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nested_virtualization: bool,
    /// GPUs for the machine: a count (`1`), `all`, or `{ count, ids, driver, capabilities,
    /// options }`. A container gets the host's (Docker, Kubernetes); a VM gets them passed
    /// through (Proxmox, by `ids`) or sized with them (a cloud's GPU instance type). Other
    /// targets refuse it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<super::GpusInput>")]
    pub gpus: Option<super::Gpus>,
    /// The host's devices given to the machine: `/dev/fuse`, `host:guest[:permissions]`, or
    /// `{ host, guest, permissions }` (`r`, `w`, `m`). A container on the user's own Docker
    /// host only: a VM has its own devices (a GPU reaches a VM through `gpus`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(with = "Vec<super::DeviceInput>")]
    pub devices: Vec<super::Device>,
    /// The machine's root filesystem is read-only (hardening): nothing can be written outside
    /// its declared `volumes`. Applies to containers; a VM's root stays writable.
    #[serde(default)]
    pub read_only: bool,
    /// Memory-backed (tmpfs) mounts inside the machine, for scratch space that never touches
    /// disk: a path (`/tmp`), or a path with a size and a mode (`{ path: /run, size: 64m, mode:
    /// "1777" }`). A VM sets these up in its own provisioning (fstab).
    #[serde(default)]
    pub tmpfs: Vec<Tmpfs>,
    /// The size of `/dev/shm` (shared memory), e.g. `256m` or `1g`, for workloads that need more
    /// than the small default (browsers, some databases). A VM sizes it in its own provisioning.
    #[serde(default)]
    pub shm_size: Option<String>,
    /// How the machine resolves names: which DNS servers to use, search domains, and its own
    /// domain. Useful to point a Linux box at the lab's domain controller. A VM sets its resolver
    /// in its own provisioning (resolv.conf).
    #[serde(default)]
    pub dns: Option<Dns>,
    /// Environment variables with fixed values: a container's environment; on a VM, every
    /// service's (systemd's default environment) and every login's (`/etc/environment`).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty", deserialize_with = "scalars")]
    #[schemars(with = "IndexMap<String, ScalarSchema>")]
    pub environment: IndexMap<String, String>,
    /// Kernel parameters, e.g. `net.core.somaxconn: 1024`. A container can only set the ones
    /// the kernel keeps per network or IPC namespace (`net.*`, `kernel.shm*`, `kernel.msg*`,
    /// `kernel.sem`, `fs.mqueue.*`); a VM sets any (`/etc/sysctl.d`).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty", deserialize_with = "scalars")]
    #[schemars(with = "IndexMap<String, ScalarSchema>")]
    pub sysctls: IndexMap<String, String>,
    /// Process limits, by name (`nofile`, `nproc`, `memlock`...): one value for both, or
    /// `{ soft, hard }`. A container's ulimits; on a VM, every service's (systemd) and every
    /// login's (`limits.d`). Kubernetes has none per container.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub ulimits: IndexMap<String, Ulimit>,
    /// The ports the machine answers on, each with its protocol, an HTTP probe and whether it
    /// is published or dormant: what is published, opened by `reach`, waited for and checked.
    #[serde(default)]
    pub ports: Vec<Port>,
    /// The spec's `inputs` this machine receives, by name: only these reach it.
    #[serde(default)]
    pub inputs: Vec<String>,
    /// The secrets this machine gets, as files (never environment variables): a name from the
    /// top-level `secrets` (`/run/secrets/<name>`), or `{ source, target, uid, gid, mode }`.
    /// Delivered by `isoloom run` when the machine starts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<MachineSecret>,
    /// CPUs, memory and disk for the machine (a VM's size, a container's limits). Default: 1
    /// CPU, 1024 MB, 20 GB.
    #[serde(default)]
    pub resources: Option<Resources>,
    /// The machine's own command, instead of its image's (`CMD`; the image's entrypoint still
    /// runs it). A text runs in the shell, a list as it is. On a VM it runs as a service
    /// (systemd, OpenRC on Alpine; a startup task on Windows), restarted when it fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandLine>,
    /// The directory the command, the `before` steps and the healthcheck run in (an absolute
    /// path). Default: the image's `WORKDIR`; on a VM, the project folder (`/opt/isoloom`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// Scripts in the project run inside the machine before its command, at every start, with
    /// its inputs and volumes (a per-player flag written where the app reads it). `.sh` (`.ps1`
    /// on a Windows VM). On a container they replace the image's entrypoint, so a container
    /// machine with `before` restates its whole process in `command`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,
    /// When the machine counts as healthy beyond its ports answering: a command or a
    /// page, with its pace. Checked beside the port probe, never instead of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<Healthcheck>,
    /// Machines that must be ready (their ports answering) before this one starts.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Data that survives restarts and re-creation of the machine, until the environment is
    /// destroyed: a name -> an absolute path in the machine. On the container targets a volume
    /// starts with the image's files at that path (Docker's named volumes; Kubernetes seeds
    /// its claim from the image while it is empty), so init jobs can add to them. A shared
    /// volume's user may take one folder of it: `{ path: /srv/www, subpath: www }`.
    #[serde(default)]
    pub volumes: IndexMap<String, Mount>,
    /// Files placed at a path on the machine (a configuration the image doesn't carry), from
    /// the project or written inline, with their owner and mode: on every target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<MachineFile>,
    /// Data disks beside the machine's own, by name (kebab-case, at most 12 characters: the
    /// filesystem's label): `{ size_gb: 50, fs: xfs, mount: /data }`. A VM gets an empty disk
    /// of that size, formatted and mounted at boot; a container gets a volume at the mount. The
    /// disks go with the machine.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub disks: IndexMap<String, Disk>,
    /// The machine the user lands on. It may have no implementation: the runner supplies it.
    #[serde(default)]
    pub access: bool,
    /// How a container produces this machine: an image or a build, and what only a container
    /// has (init jobs, appliances). A machine with `docker:` can run on the container targets.
    #[serde(default)]
    pub docker: Option<DockerImpl>,
    /// How a VM produces this machine (services installed natively, no Docker inside).
    #[serde(default)]
    pub vm: Option<VmImpl>,
    /// Where this machine already exists (the `external` target): Isoloom creates nothing and
    /// reaches it over SSH to provision, check and connect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<External>,
    /// Filled by the runner's image table (an access machine the spec leaves to the runner):
    /// a stock image, kept running idle for the user to work from. Never in a spec file.
    #[serde(skip)]
    #[schemars(skip)]
    pub supplied: bool,
}

/// A port a machine answers on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)] // the spec's own keys: each is a switch a user writes.
pub struct Port {
    /// The port the machine listens on (what the other machines and the
    /// checks connect to).
    pub port: u16,
    /// A name for the port (`web`, `api`): how outputs and messages refer to it. Default:
    /// its port.
    #[serde(default)]
    pub name: Option<String>,
    /// It speaks HTTP: derived checks request a page instead of only connecting.
    #[serde(default)]
    pub http: bool,
    /// It speaks TLS (HTTPS for an `http` port): probes and checks use TLS, certificates
    /// unchecked (lab services sign their own).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tls: bool,
    /// Reachable from the user's machine on this port (loopback only), besides the
    /// environment's own networks.
    #[serde(default)]
    pub publish: Option<u16>,
    /// Keep `publish` as the host port on local Docker too (normally a free one is picked, so
    /// labs never collide): for apps whose pages call `localhost:<that port>` themselves. The
    /// environment can't start while another holds that port.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
    /// Only answers later, once something wakes it (the bind shell an exploit opens): declared,
    /// published and opened by `reach` like any port, but nothing waits for it (healthcheck,
    /// readiness probe, `depends_on`, `up --wait`) and no derived check expects it to answer.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dormant: bool,
    /// `tcp` (default) or `udp` (DNS, SNMP, TFTP, syslog): published, forwarded and opened as
    /// UDP. UDP has no handshake, so nothing waits for it (healthcheck, readiness probe,
    /// `depends_on`) and no derived check probes it: a check of your own says how (`exec: dig`).
    /// A port served over both is two entries, one per protocol.
    #[serde(default, skip_serializing_if = "Protocol::is_tcp")]
    pub protocol: Protocol,
}

/// A port's transport protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// TCP, the default.
    #[default]
    Tcp,
    /// UDP.
    Udp,
}

impl Protocol {
    /// `tcp` or `udp`, as Compose, Vagrant, nftables and Terraform providers write it.
    pub fn id(self) -> &'static str {
        match self {
            Protocol::Tcp => "tcp",
            Protocol::Udp => "udp",
        }
    }

    /// Whether this is TCP, the default (a port without `protocol`).
    pub fn is_tcp(&self) -> bool {
        *self == Protocol::Tcp
    }
}

impl Machine {
    /// How outputs name one of its ports (published outputs, Compose labels): its `name`,
    /// else its port. A UDP port is `<port>-udp`, and `<name>-udp` when a TCP port of the
    /// machine has the same name (DNS's `dns` over both protocols), so the two never share a key.
    pub fn port_label(&self, s: &Port) -> String {
        match &s.name {
            None if s.protocol.is_tcp() => s.port.to_string(),
            None => format!("{}-udp", s.port),
            Some(n) if !s.protocol.is_tcp() && self.ports.iter().any(|o| o.protocol.is_tcp() && o.name.as_ref() == Some(n)) => {
                format!("{n}-udp")
            }
            Some(n) => n.clone(),
        }
    }

    /// The ports Isoloom waits for before the machine counts as ready: its TCP ports but the
    /// dormant ones (UDP has no handshake to probe). Empty: nothing to probe (no healthcheck, no
    /// readiness probe).
    pub fn ready_ports(&self) -> Vec<u16> {
        self.ports.iter().filter(|s| !s.dormant && s.protocol.is_tcp()).map(|s| s.port).collect()
    }

    /// Whether anything says when the machine is ready: a port to probe or a healthcheck of its
    /// own. Without either, the machine is ready once started.
    pub fn has_readiness(&self) -> bool {
        self.healthcheck.is_some() || !self.ready_ports().is_empty()
    }
}

/// What a machine is given to run on; each left out takes its default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    /// CPUs (default 1).
    #[serde(default)]
    pub cpus: Option<u32>,
    /// Memory in MB (default 1024).
    #[serde(default)]
    pub memory_mb: Option<u32>,
    /// Disk in GB, for a VM (default 20).
    #[serde(default)]
    pub disk_gb: Option<u32>,
}

/// A machine's address on a network, as written: one last octet, or several (the first the
/// primary one).
#[derive(JsonSchema)]
#[serde(untagged)]
pub enum Octets {
    /// The last octet of its address.
    One(u8),
    /// Several addresses on the network, the first its primary one.
    Several(Vec<u8>),
}

/// A text, a number or a boolean, read as text (environment values, sysctl values).
#[derive(JsonSchema)]
#[serde(untagged)]
pub enum ScalarSchema {
    /// A text.
    Text(String),
    /// A number, read as its text.
    Number(f64),
    /// A boolean, read as `true` or `false`.
    Bool(bool),
}

/// A mapping of scalars, read as texts (`1024`, `true` and `"x"` alike).
pub(crate) fn scalars<'de, D: serde::Deserializer<'de>>(d: D) -> Result<IndexMap<String, String>, D::Error> {
    let raw: IndexMap<String, serde_json::Value> = serde::Deserialize::deserialize(d)?;
    raw.into_iter()
        .map(|(k, v)| match v {
            serde_json::Value::String(s) => Ok((k, s)),
            serde_json::Value::Number(n) => Ok((k, n.to_string())),
            serde_json::Value::Bool(b) => Ok((k, b.to_string())),
            _ => Err(serde::de::Error::custom(format!("`{k}`: a text, a number or a boolean"))),
        })
        .collect()
}
