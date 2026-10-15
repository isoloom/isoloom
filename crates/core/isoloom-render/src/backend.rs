//! What every backend is: a target, the capabilities it has, and a rendering of a planned
//! [`Environment`] into files. Defined here, below every backend crate, so each can implement
//! it; `isoloom-targets` keeps the registry.

use std::fmt;

use isoloom_manifest::Manifest;
use isoloom_plan::{Capabilities, Environment, Refusal, refusal};
use isoloom_spec::model::Target;

use crate::{GenerateError, GeneratedFile};

/// The files a backend writes, paths relative to the project folder.
pub type Files = Vec<GeneratedFile>;

/// Why a backend didn't render an environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// The environment needs a capability the backend lacks.
    Refused {
        /// The backend's target.
        target: Target,
        /// The requirement it can't meet.
        refusal: Refusal,
    },
    /// A backend refused the spec for a reason of its own (a limit of its target).
    Generate(GenerateError),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::Refused { target, refusal } => write!(f, "{}: {refusal}", target.id()),
            RenderError::Generate(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<GenerateError> for RenderError {
    fn from(e: GenerateError) -> RenderError {
        RenderError::Generate(e)
    }
}

impl From<RenderError> for GenerateError {
    fn from(e: RenderError) -> GenerateError {
        match e {
            RenderError::Refused { target, refusal } => GenerateError::Unsupported {
                target,
                what: refusal.to_string(),
            },
            RenderError::Generate(e) => e,
        }
    }
}

/// A target's renderer.
pub trait Backend: Sync {
    /// The target it renders.
    fn target(&self) -> Target;

    /// What it can render.
    fn capabilities(&self) -> Capabilities;

    /// The environment's files. Refuses (without writing anything) when the environment needs
    /// a capability the backend lacks.
    fn render(&self, env: &Environment) -> Result<Files, RenderError>;

    /// Why it would refuse `env`, if it would: the plan's requirements against its
    /// capabilities, without rendering.
    fn refusal(&self, env: &Environment) -> Option<String> {
        refusal(&env.requirements, &self.capabilities()).map(|r| r.to_string())
    }

    /// The run manifest the Isothrum runner executes (`.isoloom/<target>/run.json`), for a target
    /// that has one; its `lab.message` is left to the caller, who fills it from the resolved
    /// snapshot. `None` for the targets that have none yet.
    fn run_manifest(&self, _env: &Environment) -> Result<Option<Manifest>, RenderError> {
        Ok(None)
    }
}

/// `env` checked against `caps`: the refusal as a [`RenderError`], for a backend's `render`.
pub fn check(target: Target, env: &Environment, caps: &Capabilities) -> Result<(), RenderError> {
    match refusal(&env.requirements, caps) {
        Some(refusal) => Err(RenderError::Refused { target, refusal }),
        None => Ok(()),
    }
}
