//! The Compose file Isoloom writes, as typed structs: the keys it uses, in one fixed order, each
//! left out when empty. Serialized with `serde_yaml_ng`.

mod config;
mod hardware;
mod network;
mod secret;
mod service;
mod storage;

pub(crate) use config::{Config, ConfigMount};
pub(crate) use hardware::{DeviceRequest, Reservations, ServiceDevice};
pub(crate) use network::{Attachment, Block, Ipam, Network};
pub(crate) use secret::{Secret, ServiceSecret};
pub(crate) use service::{Build, Condition, Deploy, Healthcheck, Limits, NetworkMode, Resources, Restart, Seconds, Service, Ulimit};
pub(crate) use storage::{LongMount, Mount, TmpfsOptions, Volume, VolumeOptions};

use indexmap::IndexMap;
use serde::Serialize;

/// A whole Compose file.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ComposeFile {
    /// The project name.
    pub(crate) name: String,
    /// Its services, in the order Isoloom adds them.
    pub(crate) services: IndexMap<String, Service>,
    /// Its networks.
    pub(crate) networks: IndexMap<String, Network>,
    /// Its named volumes.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) volumes: IndexMap<String, Volume>,
    /// Its secrets, each read from a variable of Compose's own environment.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) secrets: IndexMap<String, Secret>,
    /// The machines' files, each a config its service mounts.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) configs: IndexMap<String, Config>,
}
