//! What every target of this crate does around its own files: the plan's requirements first
//! (the features the target lacks), then the target's own refusal (a limit of its platform),
//! then its files.

use isoloom_plan::{Capabilities, Environment};
use isoloom_render::backend::{self, Files, RenderError};
use isoloom_spec::model::Target;

use crate::{Error, Result};

/// `target`'s files for `env`: refused when the plan asks for more than `capabilities`, or when
/// `refusal` gives a reason; else `files`.
pub(crate) fn render(
    target: Target,
    capabilities: &Capabilities,
    env: &Environment,
    refusal: impl FnOnce() -> Option<String>,
    files: impl FnOnce() -> Result<Files>,
) -> Result<Files, RenderError> {
    backend::check(target, env, capabilities)?;
    if let Some(reason) = refusal() {
        return Err(Error::Refused { target, reason }.into());
    }
    Ok(files()?)
}

/// Why a target with `capabilities` can't run `env`: the plan's requirements it lacks, else its
/// own `refusal`.
pub(crate) fn refusal(env: &Environment, capabilities: &Capabilities, refusal: impl FnOnce() -> Option<String>) -> Option<String> {
    isoloom_plan::refusal(&env.requirements, capabilities).map(|r| r.to_string()).or_else(refusal)
}
