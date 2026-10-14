//! The Compose file Isoloom writes, as typed structs: the keys it uses, in one fixed order, each
//! left out when empty. Serialized with `serde_yaml_ng`.

use indexmap::IndexMap;
use serde::Serialize;

/// A whole Compose file.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ComposeFile {
    /// The project name.
    pub name: String,
    /// Its services, in the order Isoloom adds them.
    pub services: IndexMap<String, Service>,
    /// Its networks.
    pub networks: IndexMap<String, Network>,
    /// Its named volumes.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub volumes: IndexMap<String, Labelled>,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's `skip_serializing_if` passes a reference.
fn no(b: &bool) -> bool {
    !*b
}

/// One service.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Service {
    /// Builds the image from a context instead of pulling it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<Build>,
    /// The image to run (pulled when it isn't built).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// The platform to pull or build for (`linux/arm64`), when not the host's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// Profiles the service belongs to: it starts only when one is enabled.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,
    /// Runs with every capability and the host's devices.
    #[serde(skip_serializing_if = "no")]
    pub privileged: bool,
    /// Mounts the container's root file system read-only.
    #[serde(skip_serializing_if = "no")]
    pub read_only: bool,
    /// The user the command runs as.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Kernel parameters set in the container's namespaces.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub sysctls: IndexMap<String, String>,
    /// Resource limits for the container's processes, by name (`nofile`).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub ulimits: IndexMap<String, Ulimit>,
    /// The size of `/dev/shm`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shm_size: Option<String>,
    /// Paths mounted as in-memory file systems.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tmpfs: Vec<String>,
    /// The resolvers the container uses.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dns: Vec<String>,
    /// The container resolver's options (`ndots:2`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dns_opt: Vec<String>,
    /// The search domains the container's resolver appends.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dns_search: Vec<String>,
    /// The container's domain name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domainname: Option<String>,
    /// Replaces the image's entrypoint.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entrypoint: Vec<String>,
    /// Replaces the image's command.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    /// The container's host name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Shares another container's network stack (`service:<name>`) instead of joining networks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_mode: Option<String>,
    /// `None` for a plain attachment (`net:` with no settings).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub networks: IndexMap<String, Option<Attachment>>,
    /// Kernel capabilities added to the container's defaults.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cap_add: Vec<String>,
    /// Lines added to the container's `/etc/hosts` (`name:address`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra_hosts: Vec<String>,
    /// Environment variables; `Some` of an empty map still writes the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<IndexMap<String, String>>,
    /// Ports published on the host (`address:host:container`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<String>,
    /// Mounts: bind mounts and named volumes (`source:target[:ro]`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<String>,
    /// How Docker decides the container is healthy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<Healthcheck>,
    /// Services started first, each with the condition it must reach.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub depends_on: IndexMap<String, Condition>,
    /// Resource limits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deploy: Option<Deploy>,
    /// The container's labels (Isoloom's own, to find what it started).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub labels: IndexMap<String, String>,
    /// When Docker restarts the container (`unless-stopped`, `no`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<String>,
}

impl Service {
    /// Waits for `service` to reach `condition` before starting.
    pub fn after(&mut self, service: impl Into<String>, condition: &str) {
        self.depends_on.insert(
            service.into(),
            Condition {
                condition: condition.to_string(),
            },
        );
    }
}

/// A service's `build`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Build {
    /// The build context, relative to the Compose file.
    pub context: String,
    /// The Dockerfile, when not `Dockerfile` in the context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dockerfile: Option<String>,
    /// The stage of a multi-stage Dockerfile to build, when not its last.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Build arguments.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub args: IndexMap<String, String>,
    /// Labels on the built image.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub labels: IndexMap<String, String>,
}

/// A ulimit: one number when soft and hard agree.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(untagged)]
pub enum Ulimit {
    /// Soft and hard alike.
    Both(u64),
    /// Each its own.
    Range {
        /// The limit a process starts with, and may raise up to `hard`.
        soft: u64,
        /// The ceiling only a privileged process may raise.
        hard: u64,
    },
}

/// A service's settings on one network.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Attachment {
    /// The fixed address on the network.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4_address: Option<String>,
    /// The fixed IPv6 address on an IPv6 network.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6_address: Option<String>,
    /// Other names the service answers to on the network.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// The fixed MAC address of the interface.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
    /// The interface's name in the container (`eth1`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface_name: Option<String>,
    /// The order Compose attaches networks in (the highest first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
    /// Which attachment gives the default route (the highest wins).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gw_priority: Option<u32>,
}

impl Attachment {
    /// At a fixed address.
    #[allow(clippy::needless_pass_by_value)] // takes an address or its text alike.
    pub fn at(address: impl ToString) -> Option<Attachment> {
        Some(Attachment {
            ipv4_address: Some(address.to_string()),
            ..Attachment::default()
        })
    }
}

/// A healthcheck.
#[derive(Debug, Clone, Serialize)]
pub struct Healthcheck {
    /// The test command (`CMD-SHELL` and a shell line).
    pub test: Vec<String>,
    /// The time between two tests.
    pub interval: String,
    /// How long a test may run before it counts as failed.
    pub timeout: String,
    /// Failed tests in a row before the container is unhealthy.
    pub retries: u32,
    /// A grace period after start when failures don't count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_period: Option<String>,
}

impl Healthcheck {
    /// A shell test, retried every `every` (also its timeout) up to `retries` times.
    pub fn shell(test: impl Into<String>, every: &str, retries: u32) -> Healthcheck {
        Healthcheck {
            test: vec!["CMD-SHELL".into(), test.into()],
            interval: every.into(),
            timeout: every.into(),
            retries,
            start_period: None,
        }
    }
}

/// A `depends_on` condition.
#[derive(Debug, Clone, Serialize)]
pub struct Condition {
    /// What the dependency must reach (`service_healthy`, `service_completed_successfully`).
    pub condition: String,
}

/// `deploy.resources.limits`.
#[derive(Debug, Clone, Serialize)]
pub struct Deploy {
    /// The container's resources.
    pub resources: Resources,
}

/// `deploy.resources`.
#[derive(Debug, Clone, Serialize)]
pub struct Resources {
    /// The most the container may use.
    pub limits: Limits,
}

/// `deploy.resources.limits`.
#[derive(Debug, Clone, Serialize)]
pub struct Limits {
    /// CPUs, as a decimal number of cores.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<String>,
    /// Memory, with its unit (`512M`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
}

/// A network.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Network {
    /// Created outside this file (by `name`), not by Compose.
    #[serde(skip_serializing_if = "no")]
    pub external: bool,
    /// Its name on the Docker host, when not `<project>_<key>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// IPv4 off: an IPv6-only network.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_ipv4: Option<bool>,
    /// IPv6 on: a dual-stack or IPv6-only network.
    #[serde(skip_serializing_if = "no")]
    pub enable_ipv6: bool,
    /// Its address blocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipam: Option<Ipam>,
    /// No route out of the network: its containers can't reach the outside.
    #[serde(skip_serializing_if = "no")]
    pub internal: bool,
    /// The network's labels (Isoloom's own, to find what it created).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub labels: IndexMap<String, String>,
}

/// A network's addressing.
#[derive(Debug, Clone, Serialize)]
pub struct Ipam {
    /// The blocks, one per subnet.
    pub config: Vec<Subnet>,
}

/// One block of a network.
#[derive(Debug, Clone, Serialize)]
pub struct Subnet {
    /// The block, in CIDR notation.
    pub subnet: String,
    /// The address Docker takes on the network, when not the first one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
    /// Where Docker picks the addresses it hands out, away from the fixed ones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_range: Option<String>,
}

/// A named volume: only Isoloom's labels.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Labelled {
    /// The volume's labels (Isoloom's own, to find what it created).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub labels: IndexMap<String, String>,
}
