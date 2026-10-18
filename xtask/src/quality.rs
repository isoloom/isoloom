//! `cargo xtask quality`: the engineering-standard ratchet. The measuring lives in the shared
//! `xtask-quality` crate (Isoloom and Isothrum run the same ratchet); this wrapper gives it this
//! repository's root and the two strings that differ between repositories.

use std::process::ExitCode;

pub(crate) use xtask_quality::Args;
use xtask_quality::QualityConfig;

use crate::{Res, repo};

/// This repository's ratchet configuration.
const CONFIG: QualityConfig = QualityConfig {
    jscpd_slug: "isoloom",
    update_hint: "cargo xtask quality",
};

/// Measures, then checks against (or with `--update`, rewrites) the baseline.
pub(crate) fn run(a: &Args) -> Res<ExitCode> {
    let outcome = xtask_quality::run(a, &repo::root(), &CONFIG)?;
    print!("{}", outcome.output);
    Ok(if outcome.success { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
