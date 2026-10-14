//! The spec of a project as a command sees it: with the profile asked for, the command's `-s`
//! overrides and the image table, validated, and as the instance when one is named.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use isoloom_core as core;

use crate::Res;

/// The profile asked for (`--profile`, else `ISOLOOM_PROFILE`), set once at start.
static PROFILE: OnceLock<Option<String>> = OnceLock::new();

/// Records the profile asked for: `--profile`, else `ISOLOOM_PROFILE` when it isn't empty.
pub fn set_profile(asked: Option<String>) {
    let profile = asked.or_else(|| std::env::var("ISOLOOM_PROFILE").ok().filter(|s| !s.is_empty()));
    let _ = PROFILE.set(profile);
}

/// The name of the profile asked for, recorded with an environment `run` brings up.
pub fn profile_name() -> Option<String> {
    PROFILE.get().cloned().flatten()
}

/// The profile asked for, read from `dir`. The first time, its inputs become environment
/// variables (those not already set), so every target's tool sees them as it sees the user's.
pub fn profile_for(dir: &Path) -> Res<Option<core::profile::Profile>> {
    static INPUTS: std::sync::Once = std::sync::Once::new();
    let Some(name) = PROFILE.get().and_then(Option::as_deref) else {
        return Ok(None);
    };
    let p = core::profile::load(dir, name)?;
    INPUTS.call_once(|| {
        for (k, v) in &p.inputs {
            if std::env::var_os(k).is_none() {
                // SAFETY: called while loading the spec, before the command line starts any
                // thread of its own (the check runners and log readers start later); the main
                // thread only waits for this one.
                unsafe { std::env::set_var(k, v) };
            }
        }
    });
    Ok(Some(p))
}

/// The spec in `dir`, with the profile asked for.
pub fn load_spec(dir: &Path) -> Res<core::Spec> {
    let profile = profile_for(dir)?;
    Ok(core::load_profiled(dir, profile.as_ref(), &[])?)
}

/// The target to use: the profile's when none is asked for.
pub fn pinned(dir: &Path, asked: Option<&str>) -> Res<Option<String>> {
    Ok(match profile_for(dir)? {
        Some(p) => p.pin(asked)?,
        None => asked.map(str::to_string),
    })
}

/// The spec with `-s` overrides and the image table applied, and the defaults in effect.
pub fn load_settings(dir: &Path, images: Option<&Path>, sets: &[String]) -> Res<(core::Spec, core::defaults::Resolved)> {
    let profile = profile_for(dir)?;
    let spec = core::load_profiled(dir, profile.as_ref(), sets)?;
    let settings = core::defaults::load_profiled(dir, images, profile.as_ref(), sets)?;
    let spec = settings.defaults.images.apply(&spec);
    Ok((spec, settings))
}

/// Prints the spec's problems and returns true when it has any. Generators assume a validated
/// spec (they `expect` valid CIDRs and declared networks): a spec that parses but is invalid
/// must fail with the field and reason, not a panic.
pub fn report_invalid(spec: &core::Spec) -> bool {
    let problems = core::validate(spec);
    for p in &problems {
        eprintln!("✗ {p}");
    }
    if !problems.is_empty() {
        eprintln!("fix the spec first (`isoloom validate`)");
    }
    !problems.is_empty()
}

/// Prints the spec's problems and fails when it has any: for the commands that need a valid
/// spec to say anything at all.
pub fn require_valid(spec: &core::Spec) -> Res<()> {
    let problems = core::validate(spec);
    if !problems.is_empty() {
        for p in &problems {
            eprintln!("✗ {p}");
        }
        return Err("fix the spec first (`isoloom validate`)".into());
    }
    Ok(())
}

/// The spec as instance `n` when one is named: its names, blocks and ports are the instance's.
pub fn as_instance(spec: core::Spec, instance: Option<u8>) -> Res<core::Spec> {
    Ok(match instance {
        Some(n) => core::instance::apply(&spec, n)?,
        None => spec,
    })
}

/// The spec, as the instance when one is named (its names and ports are the instance's).
pub fn load_as(dir: &Path, instance: Option<u8>) -> Res<core::Spec> {
    as_instance(load_spec(dir)?, instance)
}

/// The spec with `-s` overrides, validated, as the instance when one is named.
pub fn load_valid(dir: &Path, instance: Option<u8>, sets: &[String]) -> Res<core::Spec> {
    let (spec, _) = load_settings(dir, None, sets)?;
    require_valid(&spec)?;
    as_instance(spec, instance)
}

/// The resolved snapshot of a project (as an instance when asked), after validation.
pub fn snapshot_of(dir: &Path, instance: Option<u8>, sets: &[String]) -> Res<serde_json::Value> {
    let spec = load_valid(dir, instance, sets)?;
    Ok(core::resolved::resolve_with(&spec, instance))
}

/// A project folder as an absolute path (the registry and the tools' working directories need one).
pub fn abs(dir: &Path) -> Res<PathBuf> {
    std::fs::canonicalize(dir).map_err(|e| format!("{}: {e}", dir.display()).into())
}
