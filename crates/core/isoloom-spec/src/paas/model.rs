//! The services as written: one kind each, its portable fields, its network and its clients.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A service of the environment, by its `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Service {
    /// A managed PostgreSQL database: RDS, Azure Database for PostgreSQL flexible server, Cloud
    /// SQL; the catalogue's `postgres` (a container or a Debian VM) everywhere else.
    Postgres(Postgres),
    /// An object-storage bucket: S3, a GCS bucket, an Azure storage container; `SeaweedFS`'s S3
    /// server everywhere else (a container, or a Debian VM).
    ObjectStorage(ObjectStorage),
    /// A Kafka cluster and its topics: MSK, Event Hubs' Kafka endpoint, Google Cloud's Managed
    /// Service for Apache Kafka; Apache Kafka (`KRaft`, one node) everywhere else.
    Kafka(Kafka),
}

/// A Kafka cluster, its topics and its one SASL login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Kafka {
    /// The topics, by name, each with its partitions (default 1).
    #[serde(default)]
    pub topics: IndexMap<String, Topic>,
    /// The network it answers on, with its address there on the targets that run it as a machine.
    pub networks: IndexMap<String, u8>,
    /// The login (spec secrets): the self-hosted broker's SCRAM user and MSK's; Event Hubs and
    /// Google Cloud issue their own, delivered to the same files.
    pub credentials: KafkaCredentials,
    /// A portable size.
    #[serde(default)]
    pub size: Size,
    /// Reached from outside the environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish: Option<Publish>,
    /// The machines that use it, by name.
    #[serde(default)]
    pub clients: IndexMap<String, Client>,
}

/// A Kafka topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Topic {
    /// Its partitions.
    #[serde(default = "one")]
    pub partitions: u16,
    /// What happens to old records: deleted after the retention (`delete`, the default), or
    /// compacted to the last record of each key (`compact`).
    #[serde(default)]
    pub cleanup: Cleanup,
}

/// A topic's cleanup policy (Kafka's `cleanup.policy`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Cleanup {
    /// Records deleted after the retention.
    #[default]
    Delete,
    /// The last record of each key kept.
    Compact,
}

impl Cleanup {
    /// The policy as Kafka writes it.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Cleanup::Delete => "delete",
            Cleanup::Compact => "compact",
        }
    }
}

/// A Kafka login, each a spec secret's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KafkaCredentials {
    /// The user's secret.
    pub user: String,
    /// The password's secret.
    pub password: String,
}

/// A service reached from outside the environment, as a machine's published port is: only from
/// the addresses an input gives (the player's own, `203.0.113.7/32`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Publish {
    /// The input holding the CIDR allowed in (a `sensitive: false` input).
    pub allow: String,
}

/// How a client authenticates to a service.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    /// Keys in files: the spec's secrets, or those the platform issues.
    #[default]
    Keys,
    /// The client machine's platform identity (an instance profile, a managed identity, a
    /// service account): no key at all. Clouds only.
    Identity,
}

/// A PostgreSQL database and its one login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Postgres {
    /// The major version: one every platform offers (`"16"`).
    pub version: String,
    /// The network it answers on, with its address there on the targets that run it as a machine
    /// (`{ lab: 40 }`); a cloud picks its own address, and the service's name follows it.
    pub networks: IndexMap<String, u8>,
    /// The database created at the start.
    #[serde(default = "app")]
    pub database: String,
    /// Its owner, the one login.
    #[serde(default = "app")]
    pub user: String,
    /// The login's password: a spec secret's name, the same value on every target.
    pub password: String,
    /// A portable size, never an instance type.
    #[serde(default)]
    pub size: Size,
    /// Reached from outside the environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish: Option<Publish>,
    /// The machines that use it, by name.
    #[serde(default)]
    pub clients: IndexMap<String, Client>,
}

/// An object-storage bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectStorage {
    /// The bucket's name, 3 to 46 lowercase letters, digits and `-`. On a cloud, the copy's
    /// unique prefix goes before it (bucket names are global).
    pub bucket: String,
    /// The API its clients speak.
    #[serde(default)]
    pub api: Api,
    /// Keep every version of an object.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub versioning: bool,
    /// The network it answers on, with its address there on the targets that run it as a machine.
    pub networks: IndexMap<String, u8>,
    /// The keys of the self-hosted server (spec secrets), which checks them: its clients get them
    /// as files. A cloud issues its own, delivered to the same files.
    pub credentials: Credentials,
    /// Reached from outside the environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish: Option<Publish>,
    /// The machines that use it, by name.
    #[serde(default)]
    pub clients: IndexMap<String, Client>,
}

/// The API an object-storage bucket's clients speak.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Api {
    /// The S3 API, on every platform that has it (Azure has none: refused there).
    #[default]
    S3,
    /// Each platform's own API (S3, GCS, Azure Blob): the clients are told which.
    Platform,
}

impl Api {
    /// The API as written.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Api::S3 => "s3",
            Api::Platform => "platform",
        }
    }
}

/// A self-hosted object-storage server's keys, each a spec secret's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    /// The access key's secret.
    pub access_key: String,
    /// The secret key's secret.
    pub secret_key: String,
}

/// A portable size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Size {
    /// The smallest burstable tier of each cloud.
    #[default]
    Small,
    /// Two vCPUs and a few GB.
    Medium,
}

/// How a machine gets a service's facts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Client {
    /// The prefix of its environment variables (`DATABASE` gives `DATABASE_HOST`, ...). By
    /// default the service's name in upper snake case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_prefix: Option<String>,
    /// How it authenticates: keys in files (the default), or its platform identity.
    #[serde(default)]
    pub access: Access,
}

impl Service {
    /// The service's kind, as written.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Service::Postgres(_) => "postgres",
            Service::ObjectStorage(_) => "object-storage",
            Service::Kafka(_) => "kafka",
        }
    }

    /// Its network and its address there.
    #[must_use]
    pub fn networks(&self) -> &IndexMap<String, u8> {
        match self {
            Service::Postgres(p) => &p.networks,
            Service::ObjectStorage(o) => &o.networks,
            Service::Kafka(k) => &k.networks,
        }
    }

    /// Its clients.
    #[must_use]
    pub fn clients(&self) -> &IndexMap<String, Client> {
        match self {
            Service::Postgres(p) => &p.clients,
            Service::ObjectStorage(o) => &o.clients,
            Service::Kafka(k) => &k.clients,
        }
    }

    /// Its `publish`, when it is reached from outside.
    #[must_use]
    pub fn publish(&self) -> Option<&Publish> {
        match self {
            Service::Postgres(p) => p.publish.as_ref(),
            Service::ObjectStorage(o) => o.publish.as_ref(),
            Service::Kafka(k) => k.publish.as_ref(),
        }
    }

    /// The port it answers on, on every platform but the clouds' own endpoints.
    #[must_use]
    pub fn port(&self) -> u16 {
        match self {
            Service::Postgres(_) => 5432,
            Service::ObjectStorage(_) => 8333,
            Service::Kafka(_) => 9092,
        }
    }

    /// Whether a client uses its platform identity.
    #[must_use]
    pub fn uses_identity(&self) -> bool {
        self.clients().values().any(|c| c.access == Access::Identity)
    }

    /// The spec secrets it reads, with the field naming each.
    #[must_use]
    pub fn secrets(&self) -> Vec<(&'static str, &str)> {
        match self {
            Service::Postgres(p) => vec![("password", p.password.as_str())],
            Service::ObjectStorage(o) => vec![
                ("credentials.access_key", o.credentials.access_key.as_str()),
                ("credentials.secret_key", o.credentials.secret_key.as_str()),
            ],
            Service::Kafka(k) => vec![
                ("credentials.user", k.credentials.user.as_str()),
                ("credentials.password", k.credentials.password.as_str()),
            ],
        }
    }
}

impl Client {
    /// Its variables' prefix for service `service`.
    #[must_use]
    pub fn prefix(&self, service: &str) -> String {
        self.env_prefix.clone().unwrap_or_else(|| service.to_ascii_uppercase().replace('-', "_"))
    }
}

/// One partition, a topic's default.
fn one() -> u16 {
    1
}

/// `app`, the default database and user.
fn app() -> String {
    "app".to_owned()
}
