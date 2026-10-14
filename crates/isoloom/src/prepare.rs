//! What `run`, `down`, `reset`, `test` and `provision` do first: the target's files written
//! from the current spec, so a run or a test is never against stale output.

use std::path::{Path, PathBuf};

use isoloom_core as core;
use isoloom_core::Target;
use isoloom_core::events::ErrorCode;
use isoloom_plan::Environment;

use crate::failure::{Failure, fail};
use crate::{Res, generated, spec, target};

/// A project ready to run on a target.
pub struct Prepared {
    /// The spec as the instance, so names and ports downstream are the instance's.
    pub spec: core::Spec,
    /// The project folder, absolute.
    pub dir: PathBuf,
    pub target: Target,
    pub defaults: core::defaults::Defaults,
    /// What the target runs, planned from the spec: the runtime reads it rather than working
    /// things out again (the machines' published ports, the checks, the start order).
    pub plan: Environment,
}

/// Loads and validates the spec, applies the image table, picks the target (the one named, the
/// only possibility, or an error listing them) and writes that target's files.
pub fn prepare(dir: &Path, target: Option<&str>, images: Option<&Path>, instance: Option<u8>, sets: &[String]) -> Res<Prepared> {
    let target = spec::pinned(dir, target)?;
    let (spec, settings) = spec::load_settings(dir, images, sets)?;
    // Absolute, so the paths we hand to docker/vagrant/terraform don't depend on their working
    // directory (compose runs from `dir`, terraform and vagrant from the module folder).
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    spec::require_valid(&spec)?;
    let t = choose(target.as_deref(), &spec)?;
    let files = match instance {
        Some(n) => core::generate_instance(&spec, t, n),
        None => core::generate(&spec, t),
    }
    .map_err(|e| Failure::new(ErrorCode::GenerateFailed, format!("can't generate `{}`: {e}", t.id())))?;
    generated::write_all(&dir, &generated::finish(files, &settings.defaults, &dir)?)?;
    let spec = spec::as_instance(spec, instance)?;
    Ok(Prepared {
        plan: Environment::build(&spec, t, instance),
        spec,
        dir,
        target: t,
        defaults: settings.defaults,
    })
}

/// The target asked for, else the spec's only possible one; it must be one the spec can run on.
fn choose(asked: Option<&str>, spec: &core::Spec) -> Res<Target> {
    let possible = core::effective(spec);
    let t = match asked {
        Some(id) => target::by_id(id)?,
        None => match possible.as_slice() {
            [one] => *one,
            [] => return Err(fail(ErrorCode::TargetUnavailable, "this spec has no runnable target; see `isoloom targets`")),
            many => {
                let ids: Vec<&str> = many.iter().map(|t| t.id()).collect();
                let message = format!("several targets are possible ({}); pass one, e.g. `isoloom run {}`", ids.join(", "), ids[0]);
                return Err(fail(ErrorCode::TargetUnavailable, message));
            }
        },
    };
    if !possible.contains(&t) {
        return Err(fail(
            ErrorCode::TargetUnavailable,
            format!("this spec can't run on `{}`; see `isoloom targets`", t.id()),
        ));
    }
    Ok(t)
}
