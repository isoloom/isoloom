//! A run manifest but its header: what each target's builder fills in, then [`Body::manifest`]
//! completes the same way for every target.

use isoloom_manifest::{
    Checks, Cloud, Condition, Driver, Error as ManifestError, Expiry, Guest, Identity, Lifecycle, Machine, Manifest, ManifestVersion, Network, Output, Param,
    Recovery, SCHEMA_URL, SecretSlot, Target, Tool,
};
use isoloom_plan::Environment;

/// A run manifest but its header, which every one has the same way: what each target's builder
/// fills in, then [`Body::manifest`] completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    /// [`Manifest::tools`].
    pub tools: Vec<Tool>,
    /// [`Manifest::identity`].
    pub identity: Identity,
    /// [`Manifest::params`].
    pub params: Vec<Param>,
    /// [`Manifest::secrets`].
    pub secrets: Vec<SecretSlot>,
    /// [`Manifest::networks`].
    pub networks: Vec<Network>,
    /// [`Manifest::machines`].
    pub machines: Vec<Machine>,
    /// [`Manifest::lifecycle`].
    pub lifecycle: Lifecycle,
    /// [`Manifest::ready`].
    pub ready: Vec<Condition>,
    /// [`Manifest::outputs`].
    pub outputs: Vec<Output>,
    /// [`Manifest::checks`].
    pub checks: Checks,
    /// [`Manifest::recovery`].
    pub recovery: Option<Recovery>,
    /// [`Manifest::expiry`].
    pub expiry: Option<Expiry>,
    /// [`Manifest::guest`].
    pub guest: Option<Guest>,
}

impl Body {
    /// The run manifest of `env` on `target` (and `cloud`, for a cloud's), run by `driver`: the
    /// schema and its version, Isoloom as its generator, the environment, the driver and every
    /// host OS, then this body. Its message is left to the caller.
    ///
    /// # Errors
    ///
    /// A name the manifest refuses (the environment's, Isoloom's version).
    pub fn manifest(self, env: &Environment, target: Target, cloud: Option<Cloud>, driver: Driver) -> Result<Manifest, ManifestError> {
        let Body {
            tools,
            identity,
            params,
            secrets,
            networks,
            machines,
            lifecycle,
            ready,
            outputs,
            checks,
            recovery,
            expiry,
            guest,
        } = self;
        Ok(Manifest {
            schema: Some(SCHEMA_URL.to_owned()),
            manifest_version: ManifestVersion,
            generator: super::generator()?,
            environment: super::environment(env)?,
            target,
            cloud,
            driver,
            hosts: super::every_host(),
            tools,
            identity,
            params,
            secrets,
            networks,
            machines,
            lifecycle,
            ready,
            outputs,
            checks,
            recovery,
            expiry,
            guest,
        })
    }
}
