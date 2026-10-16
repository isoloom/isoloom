//! The Terraform half of the `cloud-docker` target: the environment's Compose file, run by
//! Docker on one cloud VM, as a module per cloud under `.isoloom/cloud-docker/<cloud>/`.
//!
//! - The Compose file is the `docker` target's (`.isoloom/docker/compose.yml` and the files it
//!   mounts), which `isoloom-targets` renders with the Compose backend before these modules: the
//!   VM runs exactly what the `docker` target runs.
//! - Each cloud brings its resources (`crate::cloud::<cloud>::docker`): its own network, a
//!   firewall letting `allowed_cidr` in for SSH and the published ports, and one Debian (Ubuntu
//!   on Oracle) VM sized for every container at once (`crate::docker_host::HostSize`).
//! - Terraform makes the host and nothing else: the runner installs Docker and runs the Compose
//!   file inside it over SSH ([`manifest`], the shared Compose-in-a-VM pieces), reading the
//!   containers there. Its only user data schedules the run's expiry shutdown.
//! - Its outputs: the host's address (`ip`) and its SSH user.
//!
//! The same Docker host on a Proxmox server is `docker-vm`'s module.

pub(crate) mod manifest;

use isoloom_plan::Environment;
use isoloom_render::backend::{Files, Placed};
use isoloom_render::container::HostSize;
use isoloom_render::hcl_builder::{Expression, Module, string};
use isoloom_render::run_manifest::guest::GuestCompose;
use isoloom_render::{GeneratedFile, OUTPUT_DIR};
use isoloom_spec::model::Target;

use super::{Cloud, ProviderConfig, aws, azure, digitalocean, gcp, linode, oci, operator};
use crate::module::providers::{self, Provider};
use crate::module::variable::Variable;
use crate::module::{self, ProjectCopy, output};
use crate::{Error, Result};

/// A cloud's own part of its `cloud-docker` module. The rest is the same on every cloud: the
/// `terraform` block, the module's head (`cloud::head`), and the Docker host's set-up once its
/// resources are there.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Driver {
    /// The provider it requires (beside `local`).
    pub(crate) requires: Provider,
    /// Its variables before the operator's: where it runs, which account.
    pub(crate) variables: fn() -> Vec<Variable>,
    /// Its VM size variable, when it has one.
    pub(crate) size: Option<SizeVariable>,
    /// Its provider's configuration.
    pub(crate) provider: ProviderConfig,
    /// Its resources, after the module's head, the host's user data (the expiry shutdown) in its
    /// VM; the host they make.
    pub(crate) resources: fn(&Environment, &mut Module, Expression) -> Host,
    /// The SSH user the runner's key logs in as (it has `sudo`).
    pub(crate) user: &'static str,
}

/// The Docker host a cloud's resources make: where the runner reaches it over SSH.
#[derive(Debug, Clone)]
pub(crate) struct Host {
    /// Its public address.
    pub(crate) address: Expression,
    /// The SSH user the operator's key logs in as (it has `sudo`).
    pub(crate) user: &'static str,
}

/// Why the clouds can't host the environment's containers, if they can't: arm64 (their images
/// and VM sizes are x86-64).
#[must_use]
pub fn refusal(env: &Environment) -> Option<String> {
    crate::limits::arm64_machine(env.spec()).map(|name| format!("machine `{name}`: arm64 on a cloud VM comes later (Graviton instances and arm64 images)"))
}

/// A Terraform module per cloud, in [`Cloud::ALL`]'s order: the Terraform side of the
/// `cloud-docker` target, after its Compose file.
///
/// # Errors
///
/// [`Error::Refused`] with the [`refusal`] when the clouds can't host the environment, or
/// [`Error::Module`] when a module can't be written.
pub fn modules(env: &Environment) -> Result<Files> {
    if let Some(reason) = refusal(env) {
        return Err(Error::Refused {
            target: Target::CloudDocker,
            reason,
        });
    }
    Cloud::ALL.into_iter().map(|cloud| module(cloud, env)).collect()
}

/// A cloud's VM size variable.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SizeVariable {
    /// Its name (`instance_type`).
    pub(crate) name: &'static str,
    /// Its default, for the containers' sizing.
    pub(crate) default: fn(HostSize) -> &'static str,
}

/// `cloud`'s own part.
const fn driver(cloud: Cloud) -> Driver {
    match cloud {
        Cloud::Aws => aws::docker::DRIVER,
        Cloud::Azure => azure::docker::DRIVER,
        Cloud::Gcp => gcp::docker::DRIVER,
        Cloud::DigitalOcean => digitalocean::docker::DRIVER,
        Cloud::Linode => linode::docker::DRIVER,
        Cloud::Oci => oci::docker::DRIVER,
    }
}

/// `cloud`'s module: how to start it, its providers and variables, the cloud's resources with
/// the expiry shutdown in the VM's user data, then the host's address and SSH user.
pub(crate) fn module(cloud: Cloud, env: &Environment) -> Result<GeneratedFile> {
    let driver = driver(cloud);
    let folder = format!("{}/{}", Target::CloudDocker.id(), cloud.id());
    let mut m = module::start(&operator::usage(&folder));
    m.block(providers::terraform(&[driver.requires]));
    let mut variables = (driver.variables)();
    if let Some(SizeVariable { name, default }) = driver.size {
        variables.push(Variable::new(name, "string").default(string(default(HostSize::of(env)))));
    }
    super::head(env, &mut m, variables, driver.provider, ProjectCopy::Runner);
    let host = (driver.resources)(env, &mut m, crate::machine::user_data::expiry_only());
    m.block(output("ip", host.address.clone()));
    m.block(output("ssh_user", string(host.user)));
    module::file(Target::CloudDocker, &folder, m, ProjectCopy::Runner)
}

/// The host's set-up script `.isoloom/cloud-docker/<cloud>/setup.sh` (the same on every cloud):
/// the Isothrum runner pipes it into `sudo sh -s` to install Docker in the VM.
fn setup_files(env: &Environment) -> Vec<GeneratedFile> {
    Cloud::ALL
        .iter()
        .map(|cloud| GeneratedFile {
            path: format!("{OUTPUT_DIR}/{}/{}/setup.sh", Target::CloudDocker.id(), cloud.id()),
            contents: isoloom_render::run_manifest::guest::setup_script(env),
        })
        .collect()
}

/// The setup scripts beside the modules, for a lab the clouds can host.
///
/// # Errors
///
/// [`Error::Refused`] with the [`refusal`] when the clouds can't host the environment.
pub fn setup(env: &Environment) -> Result<Files> {
    if let Some(reason) = refusal(env) {
        return Err(Error::Refused {
            target: Target::CloudDocker,
            reason,
        });
    }
    Ok(setup_files(env))
}

/// The run manifest of `env`'s containers on each cloud that can host them, beside its module
/// (`isoloom-targets` passes the Compose-in-a-VM `guest`).
///
/// # Errors
///
/// [`Error::Refused`] with the [`refusal`], or [`Error::Manifest`] when a manifest can't be built.
pub fn manifests(env: &Environment, guest: &GuestCompose) -> Result<Vec<Placed>> {
    if let Some(reason) = refusal(env) {
        return Err(Error::Refused {
            target: Target::CloudDocker,
            reason,
        });
    }
    Cloud::ALL
        .into_iter()
        .map(|cloud| {
            let m = manifest::manifest(env, cloud, &driver(cloud), guest).map_err(|source| Error::Manifest {
                target: Target::CloudDocker,
                source,
            })?;
            Ok(Placed::under(cloud.id(), m))
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests;
