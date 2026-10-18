//! What every cloud target's run manifest shares (`.isoloom/<target>/<cloud>/run.json`): the
//! Isothrum runner executes it, Terraform keeping only the infrastructure.
//!
//! - **Terraform** makes the machines (`init`, `apply`; `destroy`), its state kept by the runner
//!   (`identity.terraform.state: runner`), the provider's credentials given as connection slots
//!   on its steps only ([`connection`]), the region, the account and the profile as params the
//!   runner records at `up` and replays at `down`, never the profile current then.
//! - **The runner** archives the project and pipes it into each machine over SSH (`ssh.run`,
//!   retried while the machine boots); each machine sets itself up from its user data and says
//!   how far it got in its ready file, which the runner reads ([`steps`]).
//! - **The seal:** once the machines are set up, a second apply with `isoloom_sealed` closes the
//!   internet to the `internet: false` networks at the network ([`isoloom_manifest::Seal`]).
//! - **The expiry:** the runner computes the run's end (`expires_at`, Unix seconds), a tag on
//!   every resource for its reaper and the machines' own shutdown time.

pub(crate) mod connection;
pub(crate) mod steps;

use std::collections::BTreeSet;

use isoloom_manifest::{
    CheckRunner, Checks, Driver, Error as ManifestError, Expiry, ExpiryUnit, HostOs, Identity, Machine, Manifest, ManifestVersion, Output, Recovery, RelPath,
    Retries, SCHEMA_URL, SecretSlot, SnakeName, StateOwner, TerraformIdentity, Tool, Version,
};
use isoloom_plan::Environment;
use isoloom_render::run_manifest::{argv, environment, generator, networks, tool};

use super::Cloud;
use crate::module::variable::Variable;

pub(crate) use steps::Guest;

/// The oldest Terraform the cloud modules take: write-only attributes fed by ephemeral variables
/// (the secrets' values reach the clouds' stores without landing in the state).
pub(crate) const TERRAFORM_MIN: &str = "1.11.0";

/// How long a run lives when the embedder sets nothing, in minutes.
const DEFAULT_LIFE_MIN: u32 = 240;

/// One cloud module's run, as its target describes it.
#[derive(Debug, Clone)]
pub(crate) struct Frame {
    /// The target.
    pub(crate) target: isoloom_manifest::Target,
    /// The cloud.
    pub(crate) cloud: Cloud,
    /// The module's variables (the cloud's own, then the operator's).
    pub(crate) variables: Vec<Variable>,
    /// The machines the run makes.
    pub(crate) machines: Vec<Machine>,
    /// What the run gives back.
    pub(crate) outputs: Vec<Output>,
    /// The machines the runner delivers the project to and waits for.
    pub(crate) guests: Vec<Guest>,
    /// The check runners.
    pub(crate) runners: Vec<CheckRunner>,
    /// The launch's values and the spec's secrets the machines receive.
    pub(crate) slots: Vec<SecretSlot>,
}

/// The run manifest of `frame`, for `env`; its message is left to the caller.
pub(crate) fn build(env: &Environment, frame: Frame) -> Result<Manifest, ManifestError> {
    let sealed = frame.variables.iter().any(|v| v.name() == connection::SEALED);
    let mut params = connection::params(&frame.variables, frame.cloud)?;
    params.extend(steps::derived(&frame.runners)?);
    let mut secrets = connection::slots(frame.cloud, connection::terraform_steps(sealed))?;
    secrets.extend(frame.slots);
    Ok(Manifest {
        schema: Some(SCHEMA_URL.to_owned()),
        manifest_version: ManifestVersion,
        generator: generator()?,
        environment: environment(env)?,
        target: frame.target,
        cloud: Some(cloud_of(frame.cloud)),
        driver: Driver::Terraform,
        hosts: BTreeSet::from([HostOs::Linux, HostOs::Macos, HostOs::Windows]),
        tools: tools()?, // Terraform and ssh.
        identity: Identity::Terraform(TerraformIdentity {
            module: RelPath::parse(".")?,
            state: StateOwner::Runner,
            var_files: Vec::new(),
        }),
        params,
        secrets,
        networks: networks(env, isoloom_manifest::Template::text)?,
        machines: frame.machines,
        outputs: frame.outputs,
        lifecycle: steps::lifecycle(&frame.guests, sealed)?,
        ready: steps::ready(&frame.guests)?,
        checks: Checks {
            protocol: isoloom_manifest::CheckProtocol::V1,
            setup: Vec::new(),
            runners: frame.runners,
            teardown: Vec::new(),
        },
        recovery: recovery()?,
        expiry: expiry()?,
        guest: None,
    })
}

/// A cloud's recovery: one more apply after a failed one, for a cloud's eventual consistency (an
/// instance profile not usable yet, an address still being released).
pub(crate) fn recovery() -> Result<Option<Recovery>, ManifestError> {
    Ok(Some(Recovery {
        stale_markers: Vec::new(),
        steps: Vec::new(),
        retries: Retries::default(),
        apply_retries: Some(Retries::new(1)?),
    }))
}

/// The run's expiry: the runner computes it, the module tags its resources with it and the
/// machines shut themselves down then.
pub(crate) fn expiry() -> Result<Option<Expiry>, ManifestError> {
    Ok(Some(Expiry {
        tf_var: SnakeName::parse("expires_at")?,
        unit: ExpiryUnit::UnixSeconds,
        default_minutes: Some(DEFAULT_LIFE_MIN),
    }))
}

/// The manifest's name of `cloud`.
pub(crate) fn cloud_of(cloud: Cloud) -> isoloom_manifest::Cloud {
    match cloud {
        Cloud::Aws => isoloom_manifest::Cloud::Aws,
        Cloud::Azure => isoloom_manifest::Cloud::Azure,
        Cloud::Gcp => isoloom_manifest::Cloud::Gcp,
        Cloud::DigitalOcean => isoloom_manifest::Cloud::Digitalocean,
        Cloud::Linode => isoloom_manifest::Cloud::Linode,
        Cloud::Oci => isoloom_manifest::Cloud::Oci,
    }
}

/// Terraform (the host's: a cloud is reached over the internet) and `ssh`.
pub(crate) fn tools() -> Result<Vec<Tool>, ManifestError> {
    let mut terraform = tool("terraform", argv("terraform", &["version"])?)?;
    terraform.min_version = Some(Version::parse(TERRAFORM_MIN)?);
    Ok(vec![terraform, tool("ssh", argv("ssh", &["-V"])?)?])
}
