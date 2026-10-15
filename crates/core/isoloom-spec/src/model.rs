//! The isoloom format, version 1: what an environment looks like from outside (machines,
//! networks, who reaches whom, the ports that answer) and, per machine, how each kind of
//! target produces it (`docker:` and/or `vm:`).

mod boot;
mod bridged;
mod checks;
mod cloud;
mod constraints;
mod hardware;
mod implementation;
mod ipv6;
mod machine;
mod machine_file;
mod network;
mod private_dns;
mod process;
mod secrets;
mod shared;
mod storage;
mod target;
mod virtiofs;

pub use boot::{Boot, BootDevice, Firmware, FirmwareFormat, NetworkBoot};
pub(crate) use checks::is_yaml_file;
pub use checks::{Check, Declared, Expect};
pub use cloud::{CloudProvider, CloudServices};
pub use constraints::{Constraints, Mirrors, Proxy};
pub use hardware::{DEFAULT_GPU_DRIVER, Device, DeviceFull, DeviceInput, GpuCount, Gpus, GpusFull, GpusInput};
pub use implementation::{Appliance, Controller, Dns, DockerImpl, Generated, KNOWN_OS, Provision, REBOOT_STEP, VmImage, VmImpl, Winrm, is_reboot};
pub use ipv6::{Ipv6, Ipv6Addresses};
pub(crate) use machine::scalars;
pub use machine::{Arch, Machine, Octets, Port, Protocol, Resources, ScalarSchema};
pub use machine_file::{DEFAULT_FILE_MODE, MachineFile};
pub use network::{Bridged, Network, NetworkDocker, Reach, Tc, Vlan};
pub use private_dns::EnvironmentDns;
pub use process::{CommandLine, Healthcheck, HttpCheck, seconds};
pub use secrets::{MachineSecret, SECRETS_DIR, SecretMount, SecretSource, after_drive, is_drive_path, secret_variable};
pub use shared::{External, Group, Shared, SharedDocker, SharedVm, SharedVolume, TOOL_RECIPES, Tool, ULIMITS, Ulimit};
pub use storage::{Disk, Filesystem, Mount, MountPart, Tmpfs, TmpfsMount};
pub use target::{Shape, Target};
pub use virtiofs::{MAX_VIRTIOFS, Virtiofs, VirtiofsCache, mapping_id};

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An environment spec. Field order in the file is kept (machines start in the order written).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    /// The version of the `isoloom.yml` format the file is written in: `1`, the only one so far.
    pub version: u32,
    /// The environment's id, in kebab case (`supplier-portal`): it names its containers, VMs,
    /// networks and cloud resources, and the environment in `isoloom status`.
    pub name: String,
    /// The environment's networks, by name: each an address block the machines on it share.
    #[serde(default)]
    pub networks: IndexMap<String, Network>,
    /// Allowed traffic between networks; everything else between networks is blocked.
    #[serde(default)]
    pub reach: Vec<Reach>,
    /// The environment's names in a private DNS zone: off unless `private: true`. See
    /// [`EnvironmentDns`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns: Option<EnvironmentDns>,
    /// Names of the values a runner provides at launch (a token, a licence key): from the
    /// environment of `isoloom run`, or a profile's `inputs`. A machine receives the ones its own
    /// `inputs` lists. Never baked into images.
    #[serde(default)]
    pub inputs: Vec<String>,
    /// The inputs the authored spec marks `sensitive: false` (an URL, a mode): shown in a runner's
    /// events and logs. Every other input is masked. Set by lowering only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(skip)]
    pub plain_inputs: Vec<String>,
    /// Secrets, by kebab-case name: where `isoloom run` reads each value on the runner (`env`,
    /// `file`, `vault`, `sops`). A machine lists the ones it gets in its own `secrets`, as
    /// files. Values never reach a generated file, the resolved snapshot or a log.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub secrets: IndexMap<String, SecretSource>,
    /// The environment's machines, by name, started in the order written (after their
    /// `depends_on`).
    #[serde(default)]
    pub machines: IndexMap<String, Machine>,
    /// An environment of cloud services rather than machines (IAM users and roles, functions,
    /// buckets, queues...): a Terraform module in the project, applied into the user's own cloud
    /// account. A spec has either `cloud:` or machines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<CloudServices>,
    /// Volumes shared between machines: a machine volume with one of these names is this
    /// volume, wherever it mounts it. One declared here can also outlive `isoloom down`
    /// (`persistent: true`) or be one that exists already (`external: <name>`).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub volumes: IndexMap<String, SharedVolume>,
    /// What the place it runs in imposes: an HTTP proxy, registry and package mirrors, time
    /// servers. Usually set by a profile, one customer's network at a time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Constraints>,
    /// Machine fields shared by every machine (a machine's own value wins). `docker:` and `vm:`
    /// here only complete an implementation a machine declares itself.
    #[serde(default)]
    pub common: Option<Shared>,
    /// Named sets of machines sharing fields (over `common:`, under the machine's own; the most
    /// specific group wins). They also become Ansible inventory groups, and `isoloom exec`
    /// targets.
    #[serde(default)]
    pub groups: IndexMap<String, Group>,
    /// Provisioning across machines, once every machine is up: Ansible playbooks run from a
    /// controller on the environment's networks, with an inventory Isoloom writes.
    #[serde(default)]
    pub provision: Vec<Provision>,
    /// The controller Isoloom adds for `provision:` (and checks run from it): a small VM, sized
    /// and boxed like a machine. By default 1 CPU, 512 MB and a minimal Alpine box on Vagrant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<Controller>,
    /// Black-box checks that prove the behavior on every target: scripts, or declared probes
    /// (`http`, `tcp`, `exec`, `script`) run from a machine of the environment. Isoloom adds
    /// derived checks of its own from `ports` and `reach`.
    #[serde(default)]
    pub checks: Vec<Check>,
    /// Narrows the targets derived from the implementations (e.g. not tested on Proxmox yet).
    #[serde(default)]
    pub targets: Option<Vec<Target>>,
    /// Shown once the environment is up (`isoloom run`, `isoloom message`): how to start, in
    /// Markdown. `{{ machines.web.addresses.front }}`-style placeholders take any value of the
    /// resolved snapshot (`isoloom inspect` shows the paths).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Observers beside the environment, on every network, that aren't machines of it (not
    /// in `reach`, `resources` or the derived checks): a toolbox to look from (`shell`), or any
    /// container image.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub tools: IndexMap<String, Tool>,
    /// Base machine name -> its clones, for machines that had a `count`. Filled when parsing.
    #[serde(skip)]
    #[schemars(skip)]
    pub clones: IndexMap<String, Vec<String>>,
}

impl Spec {
    /// The private DNS zone's domain, when `dns.private` is on.
    #[must_use]
    pub fn private_domain(&self) -> Option<String> {
        self.dns.as_ref().and_then(|d| d.private_domain(&self.name))
    }

    /// The storage behind machine `machine`'s volume `volume`: the shared volume's own name,
    /// else one of its own (`<machine>-<volume>`).
    #[must_use]
    pub fn volume_key(&self, machine: &str, volume: &str) -> String {
        if self.volumes.contains_key(volume) {
            volume.to_owned()
        } else {
            format!("{machine}-{volume}")
        }
    }

    /// The machines that mount shared volume `volume`, in spec order.
    #[must_use]
    pub fn volume_users(&self, volume: &str) -> Vec<&str> {
        self.machines
            .iter()
            .filter(|(_, m)| m.volumes.contains_key(volume))
            .map(|(n, _)| n.as_str())
            .collect()
    }

    /// The machine holding shared volume `volume`: its `owner`, else its first user.
    #[must_use]
    pub fn volume_owner(&self, volume: &str) -> Option<&str> {
        match self.volumes.get(volume)?.owner.as_deref() {
            Some(o) => Some(o),
            None => self.volume_users(volume).first().copied(),
        }
    }

    /// The spec with `{{ cloud.outputs.<name> }}` filled in (checks and message), from a
    /// deployed cloud-services environment's outputs.
    #[must_use]
    pub fn with_cloud_outputs(&self, outputs: &IndexMap<String, String>) -> Spec {
        let fill = |t: &str| {
            outputs
                .iter()
                .fold(t.to_owned(), |acc, (k, v)| acc.replace(&format!("{{{{ cloud.outputs.{k} }}}}"), v))
        };
        let mut s = self.clone();
        for c in &mut s.checks {
            match c {
                Check::Script(p) => *p = fill(p),
                Check::Declared(d) => {
                    for f in [&mut d.http, &mut d.tcp, &mut d.exec, &mut d.body, &mut d.contains].into_iter().flatten() {
                        *f = fill(f);
                    }
                    for v in d.headers.values_mut() {
                        *v = fill(v);
                    }
                }
            }
        }
        s.message = s.message.as_deref().map(fill);
        s
    }

    /// The message of a cloud-services environment, its outputs filled in.
    #[must_use]
    pub fn cloud_message(&self, outputs: &IndexMap<String, String>) -> Option<String> {
        self.with_cloud_outputs(outputs).message
    }
}

/// Whether `value` is its type's default: a field left out when it holds it.
pub(crate) fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[cfg(test)]
mod tests;
