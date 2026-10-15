//! The `cloud-vm` target: one cloud VM per machine, a Terraform module per cloud under
//! `.isoloom/cloud-vm/<cloud>/`, and the check runners every cloud shares under
//! `.isoloom/cloud-vm/checks/`.
//!
//! Each cloud's module is a backend of its own ([`CloudVmModule`]): its refusal read from the
//! plan, then its module, rendered from the plan through the VM core (`crate::machine`). No
//! cloud stands for the others: a cloud whose refusal fires drops only its module, so a
//! feature one cloud can't do never hides another cloud's working module. The target is
//! refused only when every cloud refuses it ([`CloudVmBackend`]).
//!
//! What the clouds share lives here, by concept: what a cloud takes ([`rules`]: the features it
//! lacks, its GPU types, nesting, MTU, volumes and images), its machines' set-up hooks, their
//! interfaces after the first, their data disks, their own images, the networks routed through
//! a machine, the private DNS zone, the `reach` rules, the Windows limits, the outputs, and the
//! parts of a module around a cloud's own resources (its providers, its credentials).

pub(crate) mod disks;
pub(crate) mod gateways;
pub(crate) mod gpus;
pub(crate) mod hooks;
pub(crate) mod interfaces;
pub(crate) mod nested;
pub(crate) mod outputs;
pub(crate) mod own_image;
pub(crate) mod private_dns;
pub(crate) mod published;
pub(crate) mod reach;
pub(crate) mod rules;
pub(crate) mod windows;

use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::GeneratedFile;
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_render::hcl_builder::Module;
use isoloom_spec::model::Target;

use super::{Cloud, aws, azure, digitalocean, gcp, linode, oci};
use crate::machine::{self, controller, vms};
use crate::module::providers::{Provider, TLS};
use crate::{Result, checks, target};

/// The providers a cloud's module requires: `base` (the cloud's own, the archive's file list),
/// TLS for the controller's key, and `windows` for the Windows machines.
pub(crate) fn providers(env: &Environment, base: &[Provider], windows: &[Provider]) -> Vec<Provider> {
    let mut p = base.to_vec();
    if controller::needed(env) {
        p.push(TLS);
    }
    if vms(env).any(|(_, mp)| mp.is_windows()) {
        p.extend_from_slice(windows);
    }
    p
}

/// How a cloud's module writes its machines: a Linux VM, a Windows VM, the controller.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Machines {
    /// A Linux VM's resources and set-up.
    pub(crate) linux: fn(&Environment, &str, &mut Module),
    /// A Windows VM's resources and set-up.
    pub(crate) windows: fn(&Environment, &str, &mut Module),
    /// The controller's resources and set-up.
    pub(crate) controller: fn(&Environment, &mut Module),
}

impl Machines {
    /// Each VM in start order, then the controller when the environment needs one.
    pub(crate) fn write(&self, env: &Environment, m: &mut Module) {
        for (name, mp) in vms(env) {
            let machine = if mp.is_windows() { self.windows } else { self.linux };
            machine(env, name, m);
        }
        if controller::needed(env) {
            (self.controller)(env, m);
        }
    }
}

/// The credentials a cloud's module generates: the Windows machines' password, then the
/// controller's key.
pub(crate) fn credentials(env: &Environment, m: &mut Module) {
    machine::windows::write_password(env, m);
    controller::write_key(env, m);
}

/// What the clouds of the target can render: environment-level provisioning (a controller),
/// playbook checks (refused by each cloud's own limit until the cloud runs them), `exec` checks
/// (run on the machine), machine aliases and ulimits (the set-up steps), and gateway networks
/// and link impairment (refused by each cloud's own limits, with their own reasons: the plan's
/// requirements name other targets), data disks (a volume each, attached; raw ones left
/// unformatted) and tmpfs modes (a VM sets its tmpfs up in its provisioning).
fn capabilities() -> Capabilities {
    Capabilities::of([
        Feature::EnvironmentProvisioning,
        Feature::PlaybookChecks,
        Feature::ExecWithoutContainer,
        Feature::Gateways,
        Feature::MachineAliases,
        Feature::Ulimits,
        Feature::TrafficControl,
        Feature::Ipv6,
        // Addresses the cloud hands out: Oracle Cloud picks one in the subnet's prefix; Azure
        // refuses it on its own, and the other clouds refuse IPv6 altogether.
        Feature::Ipv6Auto,
        // Given on AWS (NitroTPM), Azure, Google Cloud and Oracle Cloud; DigitalOcean and Linode
        // refuse it on their own.
        Feature::Tpm,
        // Delivered by `isoloom run` over the machine's SSH, owners included.
        Feature::Secrets,
        Feature::SecretOwners,
        Feature::MachineProcess,
        // A GPU instance type (each cloud's own table; refused where it has none).
        Feature::VmGpus,
        // A size that nests and the cloud's switch (refused by the clouds that have none).
        Feature::NestedVirtualization,
        // The cloud's private zone (AWS, Azure, Google Cloud, Oracle Cloud), and every VM's
        // hosts file (on DigitalOcean and Linode, which have no private DNS, the hosts file only).
        Feature::PrivateDns,
        // The cloud learns them (a secondary address, an alias range) and the VM's set-up adds
        // them; DigitalOcean, which gives a Droplet one VPC address, refuses.
        Feature::VmMoreAddresses,
        // A network's MTU: Google Cloud sets it on its VPC; the other clouds refuse it on their
        // own (they have no setting for it).
        Feature::NetworkMtu,
        Feature::DataDisks,
        Feature::RawDisks,
        Feature::TmpfsModes,
    ])
}

/// The `cloud-vm` module on one cloud: its refusal, then its module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CloudVmModule(Cloud);

impl From<Cloud> for CloudVmModule {
    fn from(cloud: Cloud) -> CloudVmModule {
        CloudVmModule(cloud)
    }
}

impl CloudVmModule {
    /// The module on `cloud`.
    #[must_use]
    pub const fn new(cloud: Cloud) -> CloudVmModule {
        CloudVmModule(cloud)
    }

    /// Its cloud.
    #[must_use]
    pub const fn cloud(self) -> Cloud {
        self.0
    }

    /// Why its cloud can't run `env` beyond the plan's requirements: the cloud's own rules.
    fn own_refusal(self, env: &Environment) -> Option<String> {
        match self.0 {
            Cloud::Aws => aws::vm::refusal(env),
            Cloud::Azure => azure::vm::refusal(env),
            Cloud::Gcp => gcp::vm::refusal(env),
            Cloud::DigitalOcean => digitalocean::vm::refusal(env),
            Cloud::Linode => linode::vm::refusal(env),
            Cloud::Oci => oci::vm::refusal(env),
        }
    }

    /// Its module, `.isoloom/cloud-vm/<cloud>/main.tf`, for an environment it takes.
    fn module(self, env: &Environment) -> Result<GeneratedFile> {
        match self.0 {
            Cloud::Aws => aws::vm::module(env),
            Cloud::Azure => azure::vm::module(env),
            Cloud::Gcp => gcp::vm::module(env),
            Cloud::DigitalOcean => digitalocean::vm::module(env),
            Cloud::Linode => linode::vm::module(env),
            Cloud::Oci => oci::vm::module(env),
        }
    }
}

impl Backend for CloudVmModule {
    fn target(&self) -> Target {
        Target::CloudVm
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn render(&self, env: &Environment) -> std::result::Result<Files, RenderError> {
        target::render(Target::CloudVm, &capabilities(), env, || self.own_refusal(env), || Ok(vec![self.module(env)?]))
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        target::refusal(env, &capabilities(), || self.own_refusal(env))
    }
}

/// The `cloud-vm` backend: a module for every cloud whose model fits the environment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct CloudVmBackend;

impl CloudVmBackend {
    /// Each cloud, AWS first, with why it can't run `env` (`None`: its module is written).
    #[must_use]
    pub fn refusals(self, env: &Environment) -> Vec<(Cloud, Option<String>)> {
        Cloud::ALL.into_iter().map(|cloud| (cloud, CloudVmModule(cloud).refusal(env))).collect()
    }

    /// Why no cloud can run `env`, when none can: each cloud's own reason, the clouds sharing
    /// one named together. `None` as soon as one cloud accepts it.
    fn own_refusal(self, env: &Environment) -> Option<String> {
        let mut reasons: Vec<(Vec<&str>, String)> = Vec::new();
        for (cloud, why) in self.refusals(env) {
            let why = why?;
            match reasons.iter_mut().find(|(_, r)| *r == why) {
                Some((clouds, _)) => clouds.push(cloud.id()),
                None => reasons.push((vec![cloud.id()], why)),
            }
        }
        let parts: Vec<String> = reasons.into_iter().map(|(clouds, why)| format!("{}: {why}", clouds.join(", "))).collect();
        Some(format!("no cloud can run it ({})", parts.join("; ")))
    }

    /// The module of each cloud that takes `env`, then the check runners.
    fn files(self, env: &Environment) -> Result<Files> {
        let mut files = Vec::new();
        for (cloud, why) in self.refusals(env) {
            if why.is_none() {
                files.push(CloudVmModule(cloud).module(env)?);
            }
        }
        files.extend(checks::runners(env, Target::CloudVm.id()));
        Ok(files)
    }
}

impl Backend for CloudVmBackend {
    fn target(&self) -> Target {
        Target::CloudVm
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn render(&self, env: &Environment) -> std::result::Result<Files, RenderError> {
        target::render(Target::CloudVm, &capabilities(), env, || self.own_refusal(env), || self.files(env))
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        target::refusal(env, &capabilities(), || self.own_refusal(env))
    }
}
