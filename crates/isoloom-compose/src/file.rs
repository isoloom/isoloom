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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<Build>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,
    #[serde(skip_serializing_if = "no")]
    pub privileged: bool,
    #[serde(skip_serializing_if = "no")]
    pub read_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub sysctls: IndexMap<String, String>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub ulimits: IndexMap<String, Ulimit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shm_size: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tmpfs: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dns: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dns_search: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domainname: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entrypoint: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_mode: Option<String>,
    /// `None` for a plain attachment (`net:` with no settings).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub networks: IndexMap<String, Option<Attachment>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cap_add: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra_hosts: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<IndexMap<String, String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<Healthcheck>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub depends_on: IndexMap<String, Condition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deploy: Option<Deploy>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub labels: IndexMap<String, String>,
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
    pub context: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dockerfile: Option<String>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub args: IndexMap<String, String>,
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
    Range { soft: u64, hard: u64 },
}

/// A service's settings on one network.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Attachment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4_address: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface_name: Option<String>,
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
    pub test: Vec<String>,
    pub interval: String,
    pub timeout: String,
    pub retries: u32,
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
    pub condition: String,
}

/// `deploy.resources.limits`.
#[derive(Debug, Clone, Serialize)]
pub struct Deploy {
    pub resources: Resources,
}

/// `deploy.resources`.
#[derive(Debug, Clone, Serialize)]
pub struct Resources {
    pub limits: Limits,
}

/// `deploy.resources.limits`.
#[derive(Debug, Clone, Serialize)]
pub struct Limits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipam: Option<Ipam>,
    #[serde(skip_serializing_if = "no")]
    pub internal: bool,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub labels: IndexMap<String, String>,
}

/// A network's addressing.
#[derive(Debug, Clone, Serialize)]
pub struct Ipam {
    pub config: Vec<Subnet>,
}

/// One block of a network.
#[derive(Debug, Clone, Serialize)]
pub struct Subnet {
    pub subnet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
}

/// A named volume: only Isoloom's labels.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Labelled {
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub labels: IndexMap<String, String>,
}
