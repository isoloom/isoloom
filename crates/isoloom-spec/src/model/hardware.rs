//! What a machine asks of the hardware under it: GPUs, the host's devices, and nested
//! virtualization. Each target either gives it or refuses the spec with the reason: a machine
//! never silently runs without the hardware it asked for.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The GPUs a machine is given. A container gets the host's GPUs through its runtime; a VM gets
/// them passed through by its hypervisor (Proxmox) or sized with them (a cloud's GPU instance
/// types).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "GpusInput")]
pub struct Gpus {
    /// How many GPUs, or all the host has. Default: one, unless `ids` names them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<GpuCount>,
    /// Which GPUs, by the host's ids: a device index or UUID for a container, a PCI address
    /// (`0000:01:00.0`) or a resource mapping's name on Proxmox. Not with `count`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<String>,
    /// The GPUs' vendor driver (`nvidia`, `amd`, `intel`). Default: `nvidia`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    /// What the container may use the GPUs for, as NVIDIA's runtime names it (`compute`,
    /// `utility`, `video`, `graphics`, `display`). Default: the runtime's own. A VM gets the
    /// whole device, so every capability.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    /// Options for the container runtime's GPU driver, passed as they are (Compose only).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub options: IndexMap<String, String>,
}

/// The driver a machine's GPUs use when the spec names none.
pub const DEFAULT_GPU_DRIVER: &str = "nvidia";

impl Gpus {
    /// The driver: the spec's, else [`DEFAULT_GPU_DRIVER`].
    pub fn driver(&self) -> &str {
        self.driver.as_deref().unwrap_or(DEFAULT_GPU_DRIVER)
    }

    /// How many GPUs a fixed count gives: the count, the ids', else one. `None` for `all`.
    pub fn fixed_count(&self) -> Option<u32> {
        match self.count {
            Some(GpuCount::All) => None,
            Some(GpuCount::Count(n)) => Some(n),
            None if !self.ids.is_empty() => u32::try_from(self.ids.len()).ok(),
            None => Some(1),
        }
    }
}

/// A number of GPUs, or every one the host has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuCount {
    /// That many.
    Count(u32),
    /// `all`: every GPU of the host.
    All,
}

impl GpuCount {
    /// The word for every GPU.
    const ALL: &'static str = "all";
}

impl Serialize for GpuCount {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            GpuCount::Count(n) => s.serialize_u32(*n),
            GpuCount::All => s.serialize_str(GpuCount::ALL),
        }
    }
}

/// `gpus` as written: a count, `all`, or the full form.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum GpusInput {
    /// That many GPUs.
    Count(u32),
    /// `all`: every GPU of the host.
    Word(String),
    /// The full form.
    Full(GpusFull),
}

/// `gpus` in full, as written (`count` may be a number or `all`).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GpusFull {
    /// How many GPUs (a number, or `all`). Default: one, unless `ids` names them.
    #[serde(default)]
    #[schemars(with = "Option<CountSchema>")]
    count: Option<serde_json::Value>,
    /// Which GPUs, by the host's ids (a device index or UUID; a PCI address or a resource
    /// mapping on Proxmox). Not with `count`.
    #[serde(default)]
    ids: Vec<String>,
    /// The GPUs' vendor driver (`nvidia`, `amd`, `intel`). Default: `nvidia`.
    #[serde(default)]
    driver: Option<String>,
    /// What the container may use the GPUs for (`compute`, `utility`, `video`, `graphics`).
    #[serde(default)]
    capabilities: Vec<String>,
    /// Options for the container runtime's GPU driver (Compose only).
    #[serde(default)]
    options: IndexMap<String, String>,
}

/// A count of GPUs as the schema shows it: a number, or `all`.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)] // only describes the schema.
enum CountSchema {
    /// That many.
    Count(u32),
    /// Every GPU of the host.
    All(AllWord),
}

/// The word `all`.
#[derive(JsonSchema)]
#[allow(dead_code)] // only describes the schema.
enum AllWord {
    /// Every GPU of the host.
    #[serde(rename = "all")]
    All,
}

fn count(value: &serde_json::Value) -> Result<GpuCount, String> {
    match value {
        serde_json::Value::String(s) if s == GpuCount::ALL => Ok(GpuCount::All),
        serde_json::Value::Number(n) => n
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(GpuCount::Count)
            .ok_or_else(|| format!("`gpus.count`: a whole number or `all`, not {n}")),
        other => Err(format!("`gpus.count`: a whole number or `all`, not {other}")),
    }
}

impl TryFrom<GpusInput> for Gpus {
    type Error = String;

    fn try_from(input: GpusInput) -> Result<Gpus, String> {
        match input {
            GpusInput::Count(n) => Ok(Gpus {
                count: Some(GpuCount::Count(n)),
                ..Gpus::default()
            }),
            GpusInput::Word(w) if w == GpuCount::ALL => Ok(Gpus {
                count: Some(GpuCount::All),
                ..Gpus::default()
            }),
            GpusInput::Word(w) => Err(format!("`gpus`: a number, `all` or a mapping, not `{w}`")),
            GpusInput::Full(f) => Ok(Gpus {
                count: f.count.as_ref().map(count).transpose()?,
                ids: f.ids,
                driver: f.driver,
                capabilities: f.capabilities,
                options: f.options,
            }),
        }
    }
}

/// A device of the host given to a container, at a path inside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DeviceInput")]
pub struct Device {
    /// The device's path on the host (`/dev/fuse`).
    pub host: String,
    /// Its path inside the machine. Default: the host's path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest: Option<String>,
    /// What the machine may do with it: `r` (read), `w` (write), `m` (create the device node),
    /// in any combination. Default: `rwm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
}

impl Device {
    /// Its path inside the machine.
    pub fn guest(&self) -> &str {
        self.guest.as_deref().unwrap_or(&self.host)
    }
}

/// A device as written: `host[:guest[:permissions]]`, or a mapping.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum DeviceInput {
    /// `/dev/fuse`, `/dev/ttyUSB0:/dev/ttyS1`, `/dev/sda:/dev/xvda:r`.
    Short(String),
    /// The full form.
    Full(DeviceFull),
}

/// A device in full, as written.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceFull {
    /// The device's path on the host.
    host: String,
    /// Its path inside the machine. Default: the host's path.
    #[serde(default)]
    guest: Option<String>,
    /// `r`, `w` and `m`, in any combination. Default: `rwm`.
    #[serde(default)]
    permissions: Option<String>,
}

impl TryFrom<DeviceInput> for Device {
    type Error = String;

    fn try_from(input: DeviceInput) -> Result<Device, String> {
        match input {
            DeviceInput::Full(DeviceFull { host, guest, permissions }) => Ok(Device { host, guest, permissions }),
            DeviceInput::Short(s) => {
                let mut parts = s.split(':').map(String::from);
                let host = parts.next().unwrap_or_default();
                let device = Device {
                    host,
                    guest: parts.next(),
                    permissions: parts.next(),
                };
                match parts.next() {
                    Some(_) => Err(format!("`{s}`: a device is `host[:guest[:permissions]]`")),
                    None => Ok(device),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpus(yaml: &str) -> Result<Gpus, String> {
        serde_yaml_ng::from_str(yaml).map_err(|e| e.to_string())
    }

    #[test]
    fn gpus_read_as_a_count_all_or_in_full() {
        assert_eq!(gpus("2").unwrap().fixed_count(), Some(2));
        assert_eq!(gpus("all").unwrap().count, Some(GpuCount::All));
        let full = gpus("{ ids: ['0', '1'], driver: amd, capabilities: [compute] }").unwrap();
        assert_eq!((full.fixed_count(), full.driver()), (Some(2), "amd"));
        assert_eq!(gpus("{ count: all }").unwrap().fixed_count(), None);
        assert_eq!(gpus("{}").unwrap().fixed_count(), Some(1));
        assert_eq!(gpus("{}").unwrap().driver(), DEFAULT_GPU_DRIVER);
        assert!(gpus("some").unwrap_err().contains("a number, `all` or a mapping"));
        assert!(gpus("{ count: many }").is_err());
    }

    #[test]
    fn devices_read_short_or_in_full() {
        let d: Device = serde_yaml_ng::from_str("/dev/fuse").unwrap();
        assert_eq!((d.guest(), d.permissions.as_deref()), ("/dev/fuse", None));
        let d: Device = serde_yaml_ng::from_str("/dev/ttyUSB0:/dev/ttyS1:r").unwrap();
        assert_eq!((d.guest(), d.permissions.as_deref()), ("/dev/ttyS1", Some("r")));
        let d: Device = serde_yaml_ng::from_str("{ host: /dev/kvm, permissions: rw }").unwrap();
        assert_eq!((d.guest(), d.permissions.as_deref()), ("/dev/kvm", Some("rw")));
        assert!(serde_yaml_ng::from_str::<Device>("/a:/b:r:x").is_err());
    }

    #[test]
    fn they_serialize_in_full() {
        let d: Device = serde_yaml_ng::from_str("/dev/fuse").unwrap();
        assert_eq!(serde_yaml_ng::to_string(&d).unwrap(), "host: /dev/fuse\n");
        assert_eq!(serde_yaml_ng::to_string(&gpus("all").unwrap()).unwrap(), "count: all\n");
    }
}
