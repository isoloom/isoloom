//! The Terraform half of the `cloud-docker` target: the environment's Compose file, run by
//! Docker on one cloud VM, as a module per cloud under `.isoloom/cloud-docker/<cloud>/`.
//!
//! - The Compose file is the `docker` target's (`.isoloom/docker/compose.yml` and the files it
//!   mounts), which `isoloom-targets` renders with the Compose backend before these modules: the
//!   VM runs exactly what the `docker` target runs.
//! - Each cloud brings its resources (`crate::cloud::<cloud>::docker`): its own network, a
//!   firewall letting `allowed_cidr` in for SSH and the published ports, and one VM of
//!   [`HOST_OS`] from the cloud's image table (Ubuntu on Oracle, which has no Debian), sized for
//!   every container at once (`HostSize`): by their memory where the disk is a size of its own,
//!   else the smallest of the cloud's sizes with their CPUs, memory and disk (DigitalOcean's
//!   droplets, Linode's plans, as on `cloud-vm`). A cloud whose largest host lacks the memory or
//!   the disk they need is refused, its module left out ([`refusals`]); the target, when every
//!   cloud is.
//! - What the module shares with the cloud's `cloud-vm` module is one source, the cloud's
//!   [`Base`]: how to start it, its variables, its provider, the user its image signs in as.
//! - Terraform makes the host and nothing else: the runner installs Docker and runs the Compose
//!   file inside it over SSH ([`manifest`], the shared Compose-in-a-VM pieces), reading the
//!   containers there. Its only user data schedules the run's expiry shutdown.
//! - Its outputs: the host's address (`ip`) and its SSH user.
//!
//! The same Docker host on a Proxmox server is `docker-vm`'s module.

pub(crate) mod manifest;

use crate::hcl_builder::{Expression, Module, string};
use isoloom_plan::Environment;
use isoloom_render::backend::{Files, Placed};
use isoloom_render::container::HostSize;
use isoloom_render::run_manifest::guest::GuestCompose;
use isoloom_render::{GeneratedFile, OUTPUT_DIR};
use isoloom_spec::model::Target;

use super::vm::rules::{Need, Sizes};
use super::{Base, Cloud, aws, azure, digitalocean, gcp, linode, oci};
use crate::module::providers::{self, Provider};
use crate::module::variable::Variable;
use crate::module::{self, ProjectCopy, output};
use crate::{Error, Result};

/// The OS of the Docker host, as each cloud's image table names it: its image and the user it
/// signs in as are the cloud's for that OS, as on `cloud-vm` (Oracle Cloud runs it on Ubuntu).
pub(crate) const HOST_OS: &str = "debian-12";

/// A cloud's own part of its `cloud-docker` module. The rest is the same on every cloud: the
/// `terraform` block, the module's head (`cloud::head`), and the Docker host's set-up once its
/// resources are there.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Driver {
    /// What the cloud's modules share with its `cloud-vm` module: the usage, the variables, the
    /// provider, the images' users.
    pub(crate) base: Base,
    /// The provider it requires (beside `local`).
    pub(crate) requires: Provider,
    /// Its VM size variable, when it has one.
    pub(crate) size: Option<SizeVariable>,
    /// Its resources, after the module's head, the host's user data (the expiry shutdown) in its
    /// VM; the public address of the Docker host they make, where the runner reaches it over SSH.
    pub(crate) resources: fn(&Environment, &mut Module, Expression) -> Expression,
}

impl Driver {
    /// The SSH user the runner's key logs in as on the host (it has `sudo`, or is root).
    pub(crate) fn user(&self) -> &'static str {
        (self.base.user)(HOST_OS)
    }

    /// The cloud's variables, then its size variable sized for `env`'s containers.
    pub(crate) fn variables(&self, env: &Environment) -> Vec<Variable> {
        let mut variables = self.base.variables(env);
        if let Some(size) = self.size {
            variables.push(Variable::new(size.name, "string").default(string(size.sizes.fit(HostSize::of(env)))));
        }
        variables
    }

    /// Why the cloud's largest Docker host can't hold `env`'s containers, if it can't.
    fn short(&self, cloud: Cloud, env: &Environment) -> Option<String> {
        match self.size.map(|s| s.sizes) {
            Some(HostSizes::Table(table)) => {
                let s = HostSize::of(env);
                // The containers share the CPUs: a host with fewer than they count still runs them.
                table.short(cloud, "the containers' host", &Need::new(1, s.memory_mb, s.disk_gb))
            }
            Some(HostSizes::ByMemory(_)) | None => too_big(env),
        }
    }
}

/// Why the clouds can't host the environment's containers, if they can't: arm64 (their images
/// and VM sizes are x86-64), or no cloud's largest Docker host holds them ([`refusals`]), each
/// cloud's reason named.
#[must_use]
pub fn refusal(env: &Environment) -> Option<String> {
    crate::limits::arm64_machine(env.spec())
        .map(|name| format!("machine `{name}`: arm64 on a cloud VM comes later (Graviton instances and arm64 images)"))
        .or_else(|| super::none_takes(refusals(env)))
}

/// Each cloud, AWS first, with why its largest Docker host can't hold `env`'s containers
/// (`None`: its module is written).
#[must_use]
pub fn refusals(env: &Environment) -> Vec<(Cloud, Option<String>)> {
    Cloud::ALL.into_iter().map(|cloud| (cloud, driver(cloud).short(cloud, env))).collect()
}

/// The clouds that host `env`'s containers, AWS first.
fn hosting(env: &Environment) -> impl Iterator<Item = Cloud> {
    refusals(env).into_iter().filter(|(_, why)| why.is_none()).map(|(cloud, _)| cloud)
}

/// Why the containers don't fit a cloud's largest Docker host chosen by memory, if they don't:
/// their memory and Docker's own gigabyte, naming the container that asks for the most.
fn too_big(env: &Environment) -> Option<String> {
    let needed = HostSize::of(env).memory_mb;
    if needed <= HostSize::LARGEST_MEMORY_MB {
        return None;
    }
    let (name, most) = env
        .machines
        .iter()
        .filter(|(_, m)| m.shape == isoloom_plan::MachineShape::Container)
        .map(|(n, m)| (n, m.size().memory_mb))
        .max_by_key(|(_, mb)| *mb)?;
    Some(format!(
        "the containers' host needs {} MB of memory (Docker's 1024 MB and the containers', machine `{name}` asking for the most, {most} MB): the largest Docker host here has {} MB",
        needed,
        HostSize::LARGEST_MEMORY_MB
    ))
}

/// [`refusal`] as an error, before anything is written.
fn hosted(env: &Environment) -> Result<()> {
    match refusal(env) {
        Some(reason) => Err(Error::Refused {
            target: Target::CloudDocker,
            reason,
        }),
        None => Ok(()),
    }
}

/// A Terraform module per cloud that hosts the containers, in [`Cloud::ALL`]'s order: the
/// Terraform side of the `cloud-docker` target, after its Compose file.
///
/// # Errors
///
/// [`Error::Refused`] with the [`refusal`] when the clouds can't host the environment, or
/// [`Error::Module`] when a module can't be written.
pub fn modules(env: &Environment) -> Result<Files> {
    hosted(env)?;
    hosting(env).map(|cloud| module(cloud, env)).collect()
}

/// The Docker host's network on every cloud that makes one: its block, outside the lab's own
/// (the containers' networks live inside the host).
pub(crate) const HOST_NETWORK: &str = "10.42.0.0/16";

/// The Docker host's subnet in [`HOST_NETWORK`], where its one interface is.
pub(crate) const HOST_SUBNET: &str = "10.42.1.0/24";

/// A cloud's VM size variable.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SizeVariable {
    /// Its name (`instance_type`).
    pub(crate) name: &'static str,
    /// The sizes its default is chosen from, for the containers.
    pub(crate) sizes: HostSizes,
}

/// How a cloud's Docker host is sized for the containers.
#[derive(Debug, Clone, Copy)]
pub(crate) enum HostSizes {
    /// By their memory alone ([`HostSize::pick`]): the disk is a size of its own.
    ByMemory(fn(HostSize) -> &'static str),
    /// The cloud's size table, a size bundling its CPUs, memory and disk: the smallest that takes
    /// them all, as on `cloud-vm`.
    Table(Sizes),
}

impl HostSizes {
    /// The size for a host of `s`.
    fn fit(self, s: HostSize) -> &'static str {
        match self {
            HostSizes::ByMemory(pick) => pick(s),
            HostSizes::Table(table) => table.fit(&Need::new(s.cpus, s.memory_mb, s.disk_gb)),
        }
    }
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
    let mut m = module::start(&(driver.base.usage)(&folder));
    m.block(providers::terraform(&[driver.requires]));
    super::head(env, &mut m, driver.variables(env), driver.base.provider, ProjectCopy::Runner);
    let address = (driver.resources)(env, &mut m, crate::machine::user_data::expiry_only());
    m.block(output("ip", address));
    m.block(output("ssh_user", string(driver.user())));
    module::file(Target::CloudDocker, &folder, m, ProjectCopy::Runner)
}

/// The host's set-up script `.isoloom/cloud-docker/<cloud>/setup.sh` (the same on every cloud):
/// the runner pipes it into `sudo sh -s` to install Docker in the VM.
fn setup_files(env: &Environment) -> Vec<GeneratedFile> {
    hosting(env)
        .map(|cloud| GeneratedFile {
            path: format!("{OUTPUT_DIR}/{}/{}/setup.sh", Target::CloudDocker.id(), cloud.id()),
            contents: crate::docker_host::setup_script(env),
        })
        .collect()
}

/// The setup scripts beside the modules, for a lab the clouds can host.
///
/// # Errors
///
/// [`Error::Refused`] with the [`refusal`] when the clouds can't host the environment.
pub fn setup(env: &Environment) -> Result<Files> {
    hosted(env)?;
    Ok(setup_files(env))
}

/// The run manifest of `env`'s containers on each cloud that can host them, beside its module
/// (`isoloom-targets` passes the Compose-in-a-VM `guest`).
///
/// # Errors
///
/// [`Error::Refused`] with the [`refusal`], or [`Error::Manifest`] when a manifest can't be built.
pub fn manifests(env: &Environment, guest: &GuestCompose) -> Result<Vec<Placed>> {
    hosted(env)?;
    hosting(env)
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
