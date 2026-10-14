//! The keys a service's hardware is written with: its devices (long form) and the GPUs it
//! reserves.

use indexmap::IndexMap;
use serde::Serialize;

/// One of the host's devices, at a path inside the container (`devices`, long form).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceDevice {
    /// The device's path on the host.
    pub source: String,
    /// Its path inside the container.
    pub target: String,
    /// What the container may do with it (`r`, `w`, `m`).
    pub permissions: String,
}

/// `deploy.resources.reservations`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Reservations {
    /// Devices the container is given (its GPUs).
    pub devices: Vec<DeviceRequest>,
}

/// A device request: GPUs by count or by id, from a driver, with capabilities.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DeviceRequest {
    /// What the devices are for (`gpu` first, then the runtime's capabilities).
    pub capabilities: Vec<String>,
    /// The driver that provides them (`nvidia`).
    pub driver: String,
    /// How many: a number, or `all`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<serde_yaml_ng::Value>,
    /// Which ones, by the host's ids.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub device_ids: Vec<String>,
    /// The driver's own options.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub options: IndexMap<String, String>,
}
