//! The `cloud-vm` target: one cloud VM per machine, a Terraform module per cloud under
//! `.isoloom/cloud-vm/<cloud>/`, and the check runners every cloud shares under
//! `.isoloom/cloud-vm/checks/`.
//!
//! Each cloud's module is its own ([`CloudVmModule`]): its refusal read from the plan, then its
//! module, rendered from the plan through the VM core (`crate::machine`). No cloud stands for
//! the others: a cloud whose refusal fires drops only its module, so a
//! feature one cloud can't do never hides another cloud's working module. The target is
//! refused only when every cloud refuses it ([`CloudVmBackend`]).
//!
//! What the clouds share lives here, by concept: what a cloud takes ([`rules`]: the features it
//! lacks, its GPU types, nesting, MTU, volumes and images), its machines' set-up hooks, their
//! interfaces after the first, their data disks, their own images, the networks routed through
//! a machine, the private DNS zone's records, the `reach` rules, the Windows limits, the outputs,
//! and a module's frame around a cloud's own part (its [`Driver`]: its first lines, its
//! providers, its credentials).

pub(crate) mod controller_setup;
pub(crate) mod disks;
pub(crate) mod gateways;
pub(crate) mod gpus;
pub(crate) mod hooks;
pub(crate) mod interfaces;
pub(crate) mod manifest;
pub(crate) mod nested;
pub(crate) mod outputs;
pub(crate) mod own_image;
pub(crate) mod private_dns;
pub(crate) mod published;
pub(crate) mod reach;
pub(crate) mod rules;
pub(crate) mod seal;
pub(crate) mod windows;

use std::net::Ipv4Addr;

use isoloom_plan::{Capabilities, Environment, Feature, MachinePlan};
use isoloom_render::Error as RenderError;
use isoloom_render::GeneratedFile;
use isoloom_render::backend::{Backend, Files, Placed};
use isoloom_render::hcl_builder::Module;
use isoloom_spec::model::Target;

use super::{Base, Cloud, aws, azure, digitalocean, gcp, linode, oci};
use crate::machine::{self, controller, vms};
use crate::module::providers::{self, Provider, TLS};
use crate::module::variable::Variable;
use crate::module::{self, ProjectCopy};
use crate::{Result, checks, target};

/// A cloud's own part of its `cloud-vm` module. The module's frame is the same on every cloud:
/// its folder, its first lines, the `terraform` block, the module's head (`cloud::head`), the
/// file.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Driver {
    /// What the cloud's modules share with its `cloud-docker` module: the usage, the variables,
    /// the provider, the images' users.
    pub(crate) base: Base,
    /// The providers it requires, then those its Windows machines add.
    pub(crate) requires: (&'static [Provider], &'static [Provider]),
    /// Why it can't run the environment, if it can't.
    pub(crate) refusal: fn(&Environment) -> Option<String>,
    /// Its blocks after the module's head; refused when a machine can't be written (its user
    /// data over the cloud's cap).
    pub(crate) write: fn(&Environment, &mut Module) -> Result<()>,
}

/// The providers a cloud's module requires: `base` (the cloud's own), TLS for the controller's
/// key, and `windows` for the Windows machines (their password, the project's file list).
fn required(env: &Environment, base: &[Provider], windows: &[Provider]) -> Vec<Provider> {
    let mut p = base.to_vec();
    if controller::needed(env) {
        p.push(TLS);
    }
    if machine::windows::any(env) {
        p.extend_from_slice(windows);
    }
    p
}

/// A cloud's own variables for `env`: the driver's, then the seal's switch when a machine has
/// anything to seal.
pub(crate) fn own_variables(env: &Environment, driver: &Driver) -> Vec<Variable> {
    let mut v = (driver.base.variables)();
    v.extend(seal::variable(env));
    // cloud-vm reads the launch-time inputs in its machines' user data (the plain ones); the
    // cloud-docker host never does (its containers get them over SSH), so only cloud-vm declares
    // the `inputs` variable.
    v.extend(crate::module::inputs::variable_of(env));
    v
}

/// Each VM with its first network and its address there, the one its instance's interface is on
/// (a VM on no network has none).
pub(crate) fn at_first_address(env: &Environment) -> impl Iterator<Item = ((&str, &MachinePlan), (&str, Ipv4Addr))> {
    vms(env).filter_map(|(name, mp)| mp.addresses.first().map(|(net, a)| ((name, mp), (net.as_str(), *a))))
}

/// How a cloud's module writes a Windows VM.
pub(crate) type WriteWindows = fn(&Environment, (&str, &MachinePlan), &mut Module);

/// How a cloud's module writes its machines: a Linux VM, a Windows VM, the controller.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Machines {
    /// A Linux VM's resources and set-up.
    pub(crate) linux: fn(&Environment, (&str, &MachinePlan), &mut Module) -> Result<()>,
    /// A Windows VM's resources and set-up (its user data is fixed: the administrator, WinRM, the
    /// auto-stop, nothing a cap could refuse); `None` on a cloud whose refusal declines Windows.
    pub(crate) windows: Option<WriteWindows>,
    /// The controller's resources and set-up.
    pub(crate) controller: fn(&Environment, &mut Module) -> Result<()>,
}

impl Machines {
    /// Each VM in start order, then the controller when the environment needs one.
    ///
    /// # Errors
    ///
    /// The first machine that can't be written ([`crate::Error::Refused`]).
    pub(crate) fn write(&self, env: &Environment, m: &mut Module) -> Result<()> {
        for (name, mp) in vms(env) {
            match (mp.is_windows(), self.windows) {
                (true, Some(windows)) => windows(env, (name, mp), m),
                (true, None) => {}
                (false, _) => (self.linux)(env, (name, mp), m)?,
            }
        }
        if controller::needed(env) {
            (self.controller)(env, m)?;
        }
        Ok(())
    }
}

/// What the clouds of the target can render: the VM core's features (`machine::FEATURES`),
/// each cloud refusing on its own, with its reason, what it can't give (a router in the path
/// for `tc`, a network's MTU, a further address on DigitalOcean); and these.
fn capabilities() -> Capabilities {
    Capabilities::of(machine::FEATURES.into_iter().chain([
        // Addresses the cloud hands out: Oracle Cloud picks one in the subnet's prefix; Azure
        // refuses it on its own, and the other clouds refuse IPv6 altogether.
        Feature::Ipv6Auto,
        // Given on AWS (NitroTPM), Azure, Google Cloud and Oracle Cloud; DigitalOcean and Linode
        // refuse it on their own.
        Feature::Tpm,
        // A GPU instance type (each cloud's own table; refused where it has none).
        Feature::VmGpus,
        // A size that nests and the cloud's switch (refused by the clouds that have none).
        Feature::NestedVirtualization,
    ]))
}

/// The `cloud-vm` module on one cloud: its refusal, then its module (what [`CloudVmBackend`]
/// writes for each cloud that takes the environment).
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

    /// Its cloud's own part.
    const fn driver(self) -> Driver {
        match self.0 {
            Cloud::Aws => aws::vm::DRIVER,
            Cloud::Azure => azure::vm::DRIVER,
            Cloud::Gcp => gcp::vm::DRIVER,
            Cloud::DigitalOcean => digitalocean::vm::DRIVER,
            Cloud::Linode => linode::vm::DRIVER,
            Cloud::Oci => oci::vm::DRIVER,
        }
    }

    /// Why its cloud can't run `env` beyond the plan's requirements: the cloud's own rules, then
    /// a machine its module can't write (its user data over the cloud's cap, which only writing
    /// it measures).
    fn own_refusal(self, env: &Environment) -> Option<String> {
        let driver = self.driver();
        (driver.refusal)(env).or_else(|| match (driver.write)(env, &mut Module::default()) {
            Err(crate::Error::Refused { reason, .. }) => Some(reason),
            _ => None,
        })
    }

    /// Why its cloud can't run `env`: the plan's requirements the target lacks, else the
    /// cloud's own rules.
    #[must_use]
    pub fn refusal(self, env: &Environment) -> Option<String> {
        target::refusal(env, &capabilities(), || self.own_refusal(env))
    }

    /// Its module, `.isoloom/cloud-vm/<cloud>/main.tf`.
    ///
    /// # Errors
    ///
    /// The [`refusal`](Self::refusal) as an unsupported environment for `cloud-vm`, or the
    /// module that couldn't be written.
    pub fn render(self, env: &Environment) -> std::result::Result<GeneratedFile, RenderError> {
        target::render(Target::CloudVm, &capabilities(), env, || self.own_refusal(env), || self.module(env))
    }

    /// Its module, for an environment it takes: how to start it, the providers, then the
    /// cloud's own blocks.
    fn module(self, env: &Environment) -> Result<GeneratedFile> {
        let driver = self.driver();
        let folder = format!("{}/{}", Target::CloudVm.id(), self.0.id());
        let mut m = module::start(&(driver.base.usage)(&folder));
        let (base, windows) = driver.requires;
        m.block(providers::terraform(&required(env, base, windows)));
        // Only Terraform's WinRM set-up of a Windows machine still uploads the project (an
        // archive it reads from `local.root`); otherwise the runner delivers it and no local is
        // read.
        let copy = if machine::windows::any(env) {
            ProjectCopy::Archive
        } else {
            ProjectCopy::Runner
        };
        super::head(env, &mut m, own_variables(env, &driver), driver.base.provider, copy);
        (driver.write)(env, &mut m)?;
        module::file(Target::CloudVm, &folder, m, copy)
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

impl CloudVmModule {
    /// Its run manifest, `.isoloom/cloud-vm/<cloud>/run.json` (its message left to the caller).
    ///
    /// # Errors
    ///
    /// [`crate::Error::Manifest`] for a value the manifest's types refuse (a bug of this backend).
    fn run_manifest(self, env: &Environment) -> Result<isoloom_manifest::Manifest> {
        manifest::manifest(env, self.0, &self.driver()).map_err(|source| crate::Error::Manifest {
            target: Target::CloudVm,
            source,
        })
    }
}

impl Backend for CloudVmBackend {
    fn target(&self) -> Target {
        Target::CloudVm
    }

    /// A run manifest per cloud that takes the environment, beside its module.
    fn run_manifests(&self, env: &Environment) -> std::result::Result<Vec<Placed>, RenderError> {
        let mut out = Vec::new();
        for (cloud, why) in self.refusals(env) {
            if why.is_none() {
                out.push(Placed::under(cloud.id(), CloudVmModule(cloud).run_manifest(env)?));
            }
        }
        Ok(out)
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
