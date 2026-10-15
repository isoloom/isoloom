//! The spec as an author writes it, version 1: one declaration per machine. A machine is `os`
//! plus a catalogue `service:` or a custom `app:` (or the OS alone); per-target facts live in a
//! top-level `overrides:`. Parsed once into types that can't hold an invalid state, with every
//! problem coded and placed ([`Problem`]). The plan lowers it into [`crate::model`], the form
//! the backends read, once per target.

mod app;
mod check;
mod files;
mod forms;
mod hardware;
mod inputs;
mod machine;
mod mistake;
mod names;
mod old_keys;
mod os;
mod overrides;
mod parse;
pub mod placeholder;
mod ports;
mod problem;
mod process;
mod shared;
mod spans;
mod step;
mod warning;

pub use app::{App, ApplianceRecipe, Build, Recipe};
pub use check::{Unresolved, check, placeholder_problems};
pub use files::{File, FileSource, Mode};
pub use forms::{Form, NoForm, container as container_form, reboots, vm as vm_form};
pub use hardware::{Hardware, Tpm};
pub use inputs::{Input, Inputs};
pub use machine::{Kind, Machine, Octets, Services};
pub use names::{InputName, PortName, ServiceRef, Version};
pub use os::OsRef;
pub use overrides::{
    CloudMachine, CloudOverrides, DockerNetwork, DockerOverrides, EsxiOverrides, ExternalMachine, ExternalNetwork, ExternalOverrides, ExternalVolume,
    KubernetesOverrides, Overrides, ProviderBoxes, ProxmoxMachine, ProxmoxOverrides, ProxmoxVolume, VagrantBox, VagrantMachine, VagrantOverrides,
};
pub use parse::parse;
pub use ports::{Port, PortProtocol, Publish};
pub use problem::{Code, FieldPath, Problem, Problems, Span};
pub use process::{Command, EnvValue, HealthPage, HealthProbe, Healthcheck, PortRef};
pub use shared::{Group, Shared};
pub use spans::Spans;
pub use step::{
    AbsPath, AptWord, Argv, CommandStep, Copy, FileState, FileStep, Firewall, FirewallPort, GroupName, GroupStep, Guard, HttpsUrl, Invocation, Package,
    PackageName, PackageVersion, Password, ProjectPath, Repository, RepositoryKey, RepositoryName, Service, Sha256, Step, Template, UnitName, UserName,
    UserStep,
};
pub use warning::Warning;

use indexmap::IndexMap;
use schemars::JsonSchema;

use crate::model::{Check, CloudServices, Constraints, Controller, EnvironmentDns, Network, Provision, Reach, SecretSource, SharedVolume, Target, Tool};

/// An environment spec, as written. Field order is kept (machines start in the order written).
#[derive(Debug, Clone, PartialEq, Default, JsonSchema)]
pub struct Spec {
    /// The version of the `isoloom.yml` format the file is written in: `1`.
    pub version: Version,
    /// The environment's id, in kebab case (`supplier-portal`): it names its containers, VMs,
    /// networks and cloud resources.
    pub name: String,
    /// The environment's networks, by name: each an address block the machines on it share.
    #[schemars(default)]
    pub networks: IndexMap<String, Network>,
    /// Allowed traffic between networks; everything else between networks is blocked.
    #[schemars(default)]
    pub reach: Vec<Reach>,
    /// The environment's names in a private DNS zone: off unless `private: true`.
    pub dns: Option<EnvironmentDns>,
    /// Values given at each start (a flag, a launch token): a list of names, or a map of name to
    /// `{ sensitive, description }`. An input is sensitive unless it says otherwise: it reaches a
    /// machine as a file (`files: [{ path, input }]`), never as an environment variable.
    #[schemars(default)]
    pub inputs: Inputs,
    /// Secrets, by kebab-case name: the store each value is read from at each start (`env`,
    /// `file`, `vault`, `sops`). A machine gets one as a file (`files: [{ path, secret }]`).
    #[schemars(default)]
    pub secrets: IndexMap<String, SecretSource>,
    /// Volumes shared between machines, by name: `{ owner, persistent }`.
    #[schemars(default)]
    pub volumes: IndexMap<String, SharedVolume>,
    /// The environment's machines, by name, started in the order written (after their
    /// `depends_on`).
    #[schemars(default)]
    pub machines: IndexMap<String, Machine>,
    /// Machine fields every machine shares (a machine's own value wins).
    pub common: Option<Shared>,
    /// Named sets of machines sharing fields, also Ansible inventory groups and `overrides` keys.
    #[schemars(default)]
    pub groups: IndexMap<String, Group>,
    /// The environment's own Ansible, run from Isoloom's Linux controller once every machine is
    /// up.
    #[schemars(default)]
    pub provision: Vec<Provision>,
    /// The controller added for `provision:` and playbook checks: `{ resources, keep_running }`.
    pub controller: Option<Controller>,
    /// Black-box checks that prove the behaviour on every target.
    #[schemars(default)]
    pub checks: Vec<Check>,
    /// Shown once the environment is up, in Markdown, with `{{ path }}` placeholders:
    /// `machines.<m>.ports.<name>.port`, `.published`, `.url`, `machines.<m>.addresses.<network>`
    /// and `cloud.outputs.<name>`, each checked at validation.
    pub message: Option<String>,
    /// Per-target facts with no portable meaning, by target (`docker`, `vagrant`, `proxmox`,
    /// `aws`, ...).
    #[schemars(default)]
    pub overrides: Overrides,
    /// Observers beside the environment, on every network: a toolbox (`shell`), or any container
    /// image.
    #[schemars(default)]
    pub tools: IndexMap<String, Tool>,
    /// Narrows the targets the machines' forms allow.
    pub targets: Option<Vec<Target>>,
    /// What the place it runs in imposes: an HTTP proxy, registry and package mirrors, time
    /// servers.
    pub constraints: Option<Constraints>,
    /// An environment of cloud services rather than machines: a Terraform module applied into
    /// the user's own cloud account.
    pub cloud: Option<CloudServices>,
    /// Base machine name → its clones, for machines that had a `count`. Filled when parsing.
    #[schemars(skip)]
    pub clones: IndexMap<String, Vec<String>>,
    /// Where each field is written, for the problems found after parsing.
    #[schemars(skip)]
    pub spans: Spans,
}
