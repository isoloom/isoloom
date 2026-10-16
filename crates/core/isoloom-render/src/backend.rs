//! What every backend is: a target, the capabilities it has, and a rendering of a planned
//! [`Environment`] into files. Defined here, below every backend crate, so each can implement
//! it; `isoloom-targets` keeps the registry.

use isoloom_manifest::Manifest;
use isoloom_plan::{Capabilities, Environment, refusal};
use isoloom_spec::model::Target;

use crate::{Error, GeneratedFile, Result};

/// The files a backend writes, paths relative to the project folder.
pub type Files = Vec<GeneratedFile>;

/// A target's renderer.
pub trait Backend: Sync {
    /// The target it renders.
    fn target(&self) -> Target;

    /// What it can render.
    fn capabilities(&self) -> Capabilities;

    /// The environment's files.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] (without writing anything) when the environment needs a capability
    /// the backend lacks; [`Error::Unsupported`] for a limit of the backend's own target.
    fn render(&self, env: &Environment) -> Result<Files>;

    /// Why it would refuse `env`, if it would: the plan's requirements against its
    /// capabilities, without rendering.
    fn refusal(&self, env: &Environment) -> Option<String> {
        refusal(&env.requirements, &self.capabilities()).map(|r| r.to_string())
    }

    /// The run manifest the Isothrum runner executes (`.isoloom/<target>/run.json`), for a target
    /// that has one; its `lab.message` is left to the caller, who fills it from the resolved
    /// snapshot. `None` for the targets that have none yet.
    ///
    /// # Errors
    ///
    /// The backend's [`Error`] when the environment can't run on its target.
    fn run_manifest(&self, _env: &Environment) -> Result<Option<Manifest>> {
        Ok(None)
    }

    /// Every run manifest of the target, each with the folder it goes in under the target's own:
    /// `docker-vm` has two, one per way it makes its VM (`docker-vm/run.json` with Vagrant,
    /// `docker-vm/proxmox/run.json` with Terraform). By default, [`Backend::run_manifest`]'s, in
    /// the target's folder.
    ///
    /// # Errors
    ///
    /// The backend's [`Error`] when the environment can't run on its target.
    fn run_manifests(&self, env: &Environment) -> Result<Vec<Placed>> {
        Ok(self.run_manifest(env)?.into_iter().map(Placed::here).collect())
    }
}

/// A run manifest, and the folder it goes in under its target's (`.isoloom/<target>/[<sub>/]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// The folder under the target's, if any (`proxmox`).
    pub sub: Option<&'static str>,
    /// The manifest, its message left to the caller.
    pub manifest: Manifest,
}

impl Placed {
    /// `manifest`, in the target's own folder.
    #[must_use]
    pub fn here(manifest: Manifest) -> Placed {
        Placed { sub: None, manifest }
    }

    /// `manifest`, in the folder `sub` under the target's.
    #[must_use]
    pub fn under(sub: &'static str, manifest: Manifest) -> Placed {
        Placed { sub: Some(sub), manifest }
    }
}

/// `env` checked against `caps`, for a backend's `render`.
///
/// # Errors
///
/// [`Error::Refused`] with the first of the plan's requirements `caps` can't meet.
pub fn check(target: Target, env: &Environment, caps: &Capabilities) -> Result<()> {
    match refusal(&env.requirements, caps) {
        Some(refusal) => Err(Error::Refused { target, refusal }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use isoloom_plan::{Capabilities, Feature, Requirement};
    use isoloom_spec::model::Target;

    use super::{Backend, Files, check};
    use crate::{Error, Result, testing};

    /// A backend with no capability at all.
    struct Bare;

    impl Backend for Bare {
        fn target(&self) -> Target {
            Target::Kubernetes
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities::of([])
        }

        fn render(&self, _env: &isoloom_plan::Environment) -> Result<Files> {
            Ok(Vec::new())
        }
    }

    const ALIASED: &str =
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, aliases: [www], app: { image: nginx } }\n";

    #[test]
    fn a_backend_refuses_the_first_requirement_it_lacks_and_says_why() {
        let env = testing::env(ALIASED, Target::Kubernetes);
        let refused = check(Target::Kubernetes, &env, &Capabilities::of([]));
        assert!(
            matches!(&refused, Err(Error::Refused { target: Target::Kubernetes, refusal }) if refusal.requirement == Requirement::MachineAliases),
            "{refused:?}"
        );
        assert_eq!(check(Target::Kubernetes, &env, &Capabilities::of([Feature::MachineAliases])), Ok(()));
        assert_eq!(
            Bare.refusal(&env).as_deref(),
            Some("machine `aliases` (dotted DNS names) aren't Kubernetes Service names")
        );
        assert_eq!(Bare.run_manifest(&env), Ok(None));
        assert_eq!(Bare.run_manifests(&env), Ok(Vec::new()));
    }
}
