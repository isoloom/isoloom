//! A machine: what it is (`os` plus a catalogue `service`, a custom `app`, or nothing more), where
//! it sits on the networks, and the settings every form shares. It never says how a platform
//! runs it; each target picks the form it uses.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::Deserialize;

use super::app::App;
use super::files::File;
use super::hardware::Hardware;
use super::names::{PortName, ServiceRef};
use super::os::OsRef;
use super::ports::Port;
use super::process::{Command, EnvValue, Healthcheck};
use crate::model::{Disk, Dns, Mount, Resources, ScalarSchema, Tmpfs, Ulimit};

/// What a machine is, besides its OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A catalogue service (or several), with its settings.
    Service(Services),
    /// A custom app: an image, a build, an appliance or portable steps.
    App(App),
    /// The OS alone: its official image kept running idle, or a fresh VM.
    Bare,
}

/// Catalogue services on a machine and their settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Services {
    /// The services, each `name[@version]`.
    pub services: Vec<ServiceRef>,
    /// Their settings (`with:`), as written.
    pub with: IndexMap<String, serde_json::Value>,
}

/// A machine's address on a network: one last octet, or several (the first the primary one).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Octets {
    /// The last octet of its address (0: an address from the LAN's DHCP server, on a bridged
    /// network that hands them out).
    One(u8),
    /// Several addresses on the network, by last octet: the first is its primary address.
    Several(Vec<u8>),
}

/// A machine of the environment.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "MachineInput")]
pub struct Machine {
    /// Its OS (`None`: a container-only app, or an access machine the runner supplies).
    pub os: Option<OsRef>,
    /// What it is.
    pub kind: Kind,
    /// Network name → its address on it.
    pub networks: IndexMap<String, Octets>,
    /// More DNS names it answers to.
    pub aliases: Vec<String>,
    /// The machine the user lands on.
    pub access: bool,
    /// Its ports, by name.
    pub ports: IndexMap<PortName, Port>,
    /// Its own command.
    pub command: Option<Command>,
    /// Project scripts run once it is up.
    pub setup: Vec<String>,
    /// When it counts as healthy beyond its ports.
    pub healthcheck: Option<Healthcheck>,
    /// Machines that must be ready first.
    pub depends_on: Vec<String>,
    /// Its environment variables.
    pub environment: IndexMap<String, EnvValue>,
    /// Files placed on it.
    pub files: Vec<File>,
    /// Its volumes: name → where it mounts it.
    pub volumes: IndexMap<String, Mount>,
    /// Its data disks.
    pub disks: IndexMap<String, Disk>,
    /// Its memory-backed mounts.
    pub tmpfs: Vec<Tmpfs>,
    /// Its CPUs, memory and disk.
    pub resources: Option<Resources>,
    /// The hardware it asks for.
    pub hardware: Hardware,
    /// How it resolves names.
    pub dns: Option<Dns>,
    /// Kernel-level access.
    pub privileged: bool,
    /// A read-only root filesystem.
    pub read_only: bool,
    /// The size of `/dev/shm`.
    pub shm_size: Option<String>,
    /// Kernel parameters.
    pub sysctls: IndexMap<String, String>,
    /// Process limits.
    pub ulimits: IndexMap<String, Ulimit>,
    /// A bandwidth cap on each of its links.
    pub rate_limit: Option<String>,
}

/// A machine as written.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)] // the spec's own keys: each is a switch an author writes.
pub(crate) struct MachineInput {
    /// The OS, as `<family>@<version>` (`debian@12`, `windows-server@2022`), or the family alone
    /// for its pinned default. Required, except for a machine whose only recipe is `app.image`,
    /// `app.build` or `app.appliance`, and an `access` machine the runner supplies. Windows has
    /// no container form in 1.0.
    #[serde(default)]
    os: Option<OsRef>,
    /// A catalogue service, `name[@version]` (`postgres@16`), or a list of them. Not with `app`.
    #[serde(default, deserialize_with = "services")]
    #[schemars(with = "Option<ServicesInput>")]
    service: Option<Vec<ServiceRef>>,
    /// The service's settings, from its entry's settings schema. Only with `service`.
    #[serde(default)]
    with: IndexMap<String, serde_json::Value>,
    /// A custom app: its own image, its own Dockerfile, a network appliance, or portable steps.
    /// Not with `service`.
    #[serde(default)]
    app: Option<App>,
    /// Network name → the last octet of the machine's address on it, or a list of them (the
    /// first is its primary address). Every machine has one, but a LAN's `switch`.
    #[serde(default)]
    networks: IndexMap<String, Octets>,
    /// More DNS names it answers to besides its own (`api.example.com`), on every network.
    #[serde(default)]
    aliases: Vec<String>,
    /// How many of this machine (2 to 99): clones `<name>-01`, `<name>-02`, ..., each one address
    /// further along on every network. Where the spec names this machine, it means every clone.
    #[serde(default)]
    #[allow(dead_code)] // expanded into clones before the machines are read.
    count: Option<u8>,
    /// The machine the user lands on (at most one). Without `os`, the runner supplies it.
    #[serde(default)]
    access: bool,
    /// Its ports, by name (`site`, `dns-udp`): a number (TCP), or `{ port, protocol, publish,
    /// dormant }`. Messages and checks reach them as `machines.<m>.ports.<name>.published`.
    #[serde(default)]
    ports: IndexMap<PortName, Port>,
    /// Its own command: a text (run by the shell), a list (run as is), or `{ run, cwd }`. Default:
    /// the image's own; an `os`-only or steps-only machine stays up idle. Not with `service`.
    #[serde(default)]
    command: Option<Command>,
    /// Project scripts run once, in order, once the machine is up (as root, with its network,
    /// volumes, files and privileges): `.sh`, or `.ps1` on Windows. A machine is ready once they
    /// have run.
    #[serde(default)]
    setup: Vec<String>,
    /// When it counts as healthy beyond its ports answering: `{ test }` or `{ http: { port,
    /// path } }` (a port's name or number), with `interval`, `timeout`, `retries` and
    /// `start_period`.
    #[serde(default)]
    healthcheck: Option<Healthcheck>,
    /// Machines that must be ready (ports answering, healthcheck passed, setup run) first.
    #[serde(default)]
    depends_on: Vec<String>,
    /// Environment variables: a text, a number or a boolean, or `{ input: NAME }` for a
    /// non-sensitive input. Secrets and sensitive inputs reach a machine as `files`.
    #[serde(default)]
    environment: IndexMap<String, EnvValue>,
    /// Files placed on the machine, each from a project `source`, inline `content`, an `input` or a
    /// `secret`, with its owner and mode.
    #[serde(default)]
    files: Vec<File>,
    /// Data kept until the environment is destroyed: a name → an absolute path, or `{ path,
    /// subpath }` for one folder of a shared volume.
    #[serde(default)]
    volumes: IndexMap<String, Mount>,
    /// Data disks beside its own, by name: `{ size_gb, fs, mount }`.
    #[serde(default)]
    disks: IndexMap<String, Disk>,
    /// Memory-backed mounts: a path, or `{ path, size, mode }`.
    #[serde(default)]
    tmpfs: Vec<Tmpfs>,
    /// CPUs, memory and disk (default 1, 1024 MB, 20 GB).
    #[serde(default)]
    resources: Option<Resources>,
    /// What it asks of the hardware: `arch`, `tpm`, `gpus`, `devices`, `nested_virtualization`,
    /// `boot`.
    #[serde(default)]
    hardware: Hardware,
    /// How it resolves names: `servers` (addresses, or machines sharing a network), `search`,
    /// `domain`, `options`.
    #[serde(default)]
    dns: Option<Dns>,
    /// Kernel-level access (nested containers, kernel modules, raw devices).
    #[serde(default)]
    privileged: bool,
    /// A read-only root filesystem (containers).
    #[serde(default)]
    read_only: bool,
    /// The size of `/dev/shm` (`256m`, `1g`).
    #[serde(default)]
    shm_size: Option<String>,
    /// Kernel parameters (`net.core.somaxconn: 4096`).
    #[serde(default, deserialize_with = "crate::model::scalars")]
    #[schemars(with = "IndexMap<String, ScalarSchema>")]
    sysctls: IndexMap<String, String>,
    /// Process limits by name: one value, or `{ soft, hard }`.
    #[serde(default)]
    ulimits: IndexMap<String, Ulimit>,
    /// A bandwidth cap on each of its links, as `tc` writes a rate (`10mbit`).
    #[serde(default)]
    rate_limit: Option<String>,
}

/// `service:` as written, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)] // the schema's shape only.
enum ServicesInput {
    /// One service.
    One(ServiceRef),
    /// Several services on one machine (no container form: one process per container).
    Several(Vec<ServiceRef>),
}

fn services<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Vec<ServiceRef>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        One(ServiceRef),
        Several(Vec<ServiceRef>),
    }
    Ok(Some(match Raw::deserialize(d)? {
        Raw::One(s) => vec![s],
        Raw::Several(s) => s,
    }))
}

impl TryFrom<MachineInput> for Machine {
    type Error = String;

    fn try_from(m: MachineInput) -> Result<Machine, String> {
        let kind = match (m.service, m.app) {
            (Some(_), Some(_)) => return Err("`service` and `app` are both given: a machine is a catalogue service or a custom app".into()),
            (Some(services), None) => Kind::Service(Services { services, with: m.with }),
            (None, _) if !m.with.is_empty() => return Err("`with` sets a catalogue service's settings: it goes with `service`".into()),
            (None, Some(app)) => Kind::App(app),
            (None, None) => Kind::Bare,
        };
        Ok(Machine {
            os: m.os,
            kind,
            networks: m.networks,
            aliases: m.aliases,
            access: m.access,
            ports: m.ports,
            command: m.command,
            setup: m.setup,
            healthcheck: m.healthcheck,
            depends_on: m.depends_on,
            environment: m.environment,
            files: m.files,
            volumes: m.volumes,
            disks: m.disks,
            tmpfs: m.tmpfs,
            resources: m.resources,
            hardware: m.hardware,
            dns: m.dns,
            privileged: m.privileged,
            read_only: m.read_only,
            shm_size: m.shm_size,
            sysctls: m.sysctls,
            ulimits: m.ulimits,
            rate_limit: m.rate_limit,
        })
    }
}

impl Machine {
    /// Its custom app, when it is one.
    pub fn app(&self) -> Option<&App> {
        match &self.kind {
            Kind::App(a) => Some(a),
            _ => None,
        }
    }
}

impl JsonSchema for Machine {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Machine".into()
    }

    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        MachineInput::json_schema(g)
    }
}

#[cfg(test)]
mod tests;
