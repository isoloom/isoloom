//! A pod's containers and the volumes they mount.

use serde::Serialize;

use super::{Image, NameValue};

/// One container.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Container {
    /// Its name in the pod.
    pub(crate) name: String,
    /// What it runs.
    pub(crate) image: Image,
    /// `IfNotPresent` for a machine's image (a built one is only local).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) image_pull_policy: Option<&'static str>,
    /// Its command, instead of the image's entrypoint.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) command: Vec<String>,
    /// Its arguments, instead of the image's command.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) args: Vec<String>,
    /// The directory it starts in, instead of the image's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) working_dir: Option<String>,
    /// Its privileges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) security_context: Option<ContainerSecurity>,
    /// The ports it serves.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) ports: Vec<ContainerPort>,
    /// When it has started: probed before the readiness probe begins.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) startup_probe: Option<Probe>,
    /// When it is ready.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) readiness_probe: Option<Probe>,
    /// Its environment.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) env: Vec<EnvVar>,
    /// Its limits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) resources: Option<Resources>,
    /// Where it sees the pod's volumes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) volume_mounts: Vec<VolumeMount>,
    /// Where it sees the pod's block devices (raw data disks).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) volume_devices: Vec<VolumeDevice>,
}

impl Container {
    /// A container running `image`, nothing else set.
    pub(crate) fn new(name: impl Into<String>, image: Image) -> Container {
        Container {
            name: name.into(),
            image,
            image_pull_policy: None,
            command: Vec::new(),
            args: Vec::new(),
            working_dir: None,
            security_context: None,
            ports: Vec::new(),
            startup_probe: None,
            readiness_probe: None,
            env: Vec::new(),
            resources: None,
            volume_mounts: Vec::new(),
            volume_devices: Vec::new(),
        }
    }
}

/// A container's privileges.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContainerSecurity {
    /// Every capability, every device.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) privileged: bool,
    /// A read-only image.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) read_only_root_filesystem: bool,
    /// The user it runs as.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) run_as_user: Option<u32>,
    /// The group it runs as.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) run_as_group: Option<u32>,
}

/// A port a container serves.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContainerPort {
    /// The port.
    pub(crate) container_port: u16,
    /// `UDP`; TCP (the default) is left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) protocol: Option<&'static str>,
}

/// A probe: a command or a page, retried.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Probe {
    /// The command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exec: Option<Exec>,
    /// The page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) http_get: Option<HttpGet>,
    /// Seconds after start before the first try.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) initial_delay_seconds: Option<u32>,
    /// Seconds between tries.
    pub(crate) period_seconds: u32,
    /// Seconds one try may take.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) timeout_seconds: Option<u32>,
    /// Passing tries in a row before the pod is ready.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) success_threshold: Option<u32>,
    /// Tries before the pod is unready.
    pub(crate) failure_threshold: u32,
}

/// A page a probe requests from the pod.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct HttpGet {
    /// The page's path.
    pub(crate) path: String,
    /// The port it is served on.
    pub(crate) port: u16,
}

/// A command run in the container.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Exec {
    /// The command, exec form.
    pub(crate) command: Vec<String>,
}

/// An environment variable: fixed, or read from a Secret.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub(crate) enum EnvVar {
    /// A fixed value.
    Value(NameValue),
    /// A key of a Secret.
    Secret {
        /// The variable.
        name: String,
        /// Where its value comes from.
        #[serde(rename = "valueFrom")]
        value_from: SecretSource,
    },
}

/// Where a variable's value comes from: a Secret's key.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct SecretSource {
    /// The key.
    #[serde(rename = "secretKeyRef")]
    pub(crate) secret_key_ref: SecretKey,
}

/// A Secret's key, optional (unset is empty).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct SecretKey {
    /// The Secret.
    pub(crate) name: String,
    /// The key.
    pub(crate) key: String,
    /// Whether the pod starts without it.
    pub(crate) optional: bool,
}

/// A container's limits.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Resources {
    /// CPU and memory at most.
    pub(crate) limits: Limits,
}

/// CPU and memory limits, and the devices a device plugin offers (`nvidia.com/gpu`).
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Limits {
    /// CPUs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cpu: Option<String>,
    /// Memory (`512Mi`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) memory: Option<String>,
    /// A count of each device plugin resource, by its name.
    #[serde(flatten)]
    pub(crate) devices: indexmap::IndexMap<String, String>,
}

/// Where a container sees a volume.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VolumeMount {
    /// The pod's volume.
    pub(crate) name: String,
    /// Where.
    pub(crate) mount_path: String,
    /// Whether it can't write to it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) read_only: bool,
    /// The one file or folder of the volume it sees there, instead of the whole volume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sub_path: Option<String>,
}

/// Where a container sees a block device (a claim of `volumeMode: Block`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VolumeDevice {
    /// The pod's volume.
    pub(crate) name: String,
    /// The device's path in the container.
    pub(crate) device_path: String,
}

impl VolumeMount {
    /// Volume `name` at `path`, writable.
    pub(crate) fn at(name: impl Into<String>, path: impl Into<String>) -> VolumeMount {
        VolumeMount {
            name: name.into(),
            mount_path: path.into(),
            read_only: false,
            sub_path: None,
        }
    }

    /// Volume `name` at `path`, read-only.
    pub(crate) fn read_only(name: impl Into<String>, path: impl Into<String>) -> VolumeMount {
        VolumeMount {
            read_only: true,
            ..VolumeMount::at(name, path)
        }
    }
}

/// A pod's volume.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Volume {
    /// Its name in the pod.
    pub(crate) name: String,
    /// What it is.
    #[serde(flatten)]
    pub(crate) source: VolumeSource,
}

/// What a pod's volume is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum VolumeSource {
    /// A claim.
    PersistentVolumeClaim {
        /// The claim's name.
        #[serde(rename = "claimName")]
        claim_name: String,
    },
    /// A `ConfigMap`'s files.
    ConfigMap {
        /// The `ConfigMap`.
        name: String,
        /// Their mode (executable).
        #[serde(rename = "defaultMode")]
        default_mode: u32,
    },
    /// Scratch space, on disk or in memory.
    EmptyDir(EmptyDir),
    /// Some keys of a Secret, as files.
    Secret {
        /// The Secret.
        #[serde(rename = "secretName")]
        secret_name: String,
        /// Its keys, each with its file's name and mode.
        items: Vec<KeyToPath>,
    },
}

/// A key of a Secret as a file of its volume.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct KeyToPath {
    /// The key.
    pub(crate) key: String,
    /// The file's name in the volume.
    pub(crate) path: String,
    /// The file's mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mode: Option<u32>,
}

/// Scratch space; empty is on the node's disk (`{}`).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmptyDir {
    /// `Memory`: a tmpfs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) medium: Option<&'static str>,
    /// Its size at most.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) size_limit: Option<String>,
}
