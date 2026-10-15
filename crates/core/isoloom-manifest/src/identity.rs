//! Which output a manifest is, the tools it needs, and how the runner finds what a run created
//! without reading the generated files: the Compose project, the Vagrant providers, the
//! Terraform module and state.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::argv::HostArgv;
use crate::list::NonEmpty;
use crate::names::{PinnedImage, ToolName, Version};
use crate::path::{PathTemplate, RelPath};
use crate::template::Template;

/// The target whose files the manifest sits next to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    /// Docker Compose on the host.
    Docker,
    /// Docker Compose in one VM.
    DockerVm,
    /// One VM per machine, through Vagrant.
    Vagrant,
    /// VMs on a Proxmox server, through Terraform.
    Proxmox,
    /// VMs on an ESXi server.
    Esxi,
    /// Docker Compose on a cloud VM.
    CloudDocker,
    /// One cloud VM per machine.
    CloudVm,
    /// A cloud's own services, through the lab's Terraform module.
    CloudServices,
}

/// The cloud of a cloud target's module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Cloud {
    /// Amazon Web Services.
    Aws,
    /// Microsoft Azure.
    Azure,
    /// Google Cloud.
    Gcp,
    /// DigitalOcean.
    Digitalocean,
    /// Akamai Linode.
    Linode,
    /// Oracle Cloud Infrastructure.
    Oci,
}

/// The runner driver that implements the manifest's actions, reads its status and clears its
/// leftovers. The native commands themselves are in `lifecycle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Driver {
    /// Docker Compose.
    Compose,
    /// Vagrant.
    Vagrant,
    /// Terraform (or OpenTofu).
    Terraform,
}

impl Driver {
    /// The driver as the manifest writes it.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Driver::Compose => "compose",
            Driver::Vagrant => "vagrant",
            Driver::Terraform => "terraform",
        }
    }
}

/// A host operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HostOs {
    /// Linux.
    Linux,
    /// macOS.
    Macos,
    /// Windows.
    Windows,
}

/// A Vagrant provider (`vagrant up --provider`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// VirtualBox.
    Virtualbox,
    /// VMware Workstation or Fusion.
    VmwareDesktop,
    /// Hyper-V.
    Hyperv,
    /// Parallels Desktop.
    Parallels,
    /// libvirt (KVM).
    Libvirt,
    /// QEMU.
    Qemu,
    /// UTM.
    Utm,
    /// An ESXi server.
    VmwareEsxi,
}

/// A native tool the steps run, probed before `up`: a missing one fails with its name, not a raw
/// spawn error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    /// The tool's name, as messages show it.
    pub name: ToolName,
    /// A read-only command that succeeds when the tool (and its daemon, for Docker) is usable.
    /// Time-bounded by the runner.
    pub probe: HostArgv,
    /// The oldest version that works.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_version: Option<Version>,
    /// An image the runner may run the tool from instead of the host binary, pinned by digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<PinnedImage>,
    /// The hosts that use `container`.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub container_hosts: BTreeSet<HostOs>,
    /// Only on these hosts (empty: every host).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub hosts: BTreeSet<HostOs>,
    /// Only with these providers (empty: every provider).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub providers: BTreeSet<Provider>,
}

/// The Compose project of a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComposeIdentity {
    /// The project name: `${run.name}`, which the embedder may set (the launcher's
    /// `cyberctf-<lab id>`).
    pub project: Template,
    /// The Compose files, in order.
    pub files: NonEmpty<PathTemplate>,
    /// The runner's override file of host ports pinned at the first start (in the run's state
    /// folder), named in every Compose argv. Always written, `services: {}` when nothing is
    /// pinned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_ports_file: Option<PathTemplate>,
}

/// Whether a provider's commands run elevated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Elevation {
    /// Never.
    #[default]
    None,
    /// On Windows, management commands run elevated (one prompt per operation) when the runner
    /// may not manage Hyper-V itself. Status reads never are.
    Hyperv,
}

/// A provider the Vagrantfile supports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VagrantProvider {
    /// The provider.
    pub id: Provider,
    /// The hosts it exists on.
    pub hosts: BTreeSet<HostOs>,
    /// Its Vagrant plugin, when it needs one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<ToolName>,
    /// Whether its commands need elevation.
    #[serde(default)]
    pub elevation: Elevation,
    /// The VMs run on a server (ESXi): its connection slots apply.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub remote: bool,
    /// Why this lab can't run on this provider, in Isoloom's words, or null.
    #[serde(default)]
    pub refusal: Option<String>,
}

/// The Vagrant providers of a run, and where Vagrant keeps its record of the run's machines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VagrantIdentity {
    /// The providers the Vagrantfile supports.
    pub providers: Vec<VagrantProvider>,
    /// The folder Vagrant keeps its machines' state in (`VAGRANT_DOTFILE_PATH`, which the runner
    /// sets for every Vagrant command of the run, its actions' included), in the run's state
    /// folder (`${run.state_dir}/vagrant`): a reinstalled or replaced lab folder never loses
    /// track of its VMs. Absent: Vagrant's own `.vagrant` next to the Vagrantfile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dotfile: Option<PathTemplate>,
}

/// Who keeps a Terraform state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum StateOwner {
    /// The runner: local state in the run's state folder, outside the lab's files (a reinstalled
    /// lab must not orphan what it created).
    Runner,
}

/// The Terraform module of a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TerraformIdentity {
    /// The module's folder.
    pub module: RelPath,
    /// Who keeps the state.
    pub state: StateOwner,
    /// Variable files Isoloom generated, in order (`terraform.tfvars.json`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub var_files: Vec<RelPath>,
}

/// How the runner finds what a run created, without reading the generated files: one key, the
/// driver's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Identity {
    /// The Compose project.
    Compose(ComposeIdentity),
    /// The Vagrant providers.
    Vagrant(VagrantIdentity),
    /// The Terraform module and state.
    Terraform(TerraformIdentity),
}

impl Identity {
    /// The driver this identity is for.
    #[must_use]
    pub fn driver(&self) -> Driver {
        match self {
            Identity::Compose(_) => Driver::Compose,
            Identity::Vagrant(_) => Driver::Vagrant,
            Identity::Terraform(_) => Driver::Terraform,
        }
    }
}
