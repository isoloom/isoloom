//! The spec of a project as a command sees it: with the profile asked for, the command's `-s`
//! overrides and the image table, validated, and as the instance when one is named.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use isoloom_runtime::events::ErrorCode;

use crate::Res;
use crate::failure::Failure;

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

/// The profile's inputs every tool started from here gets, recorded when the profile is first
/// read: those not already set in this process's environment (the user's own value wins).
static INPUTS: OnceLock<Vec<(String, String)>> = OnceLock::new();

/// The profile asked for, read from `dir`. Its inputs become [`child_env`] the first time.
pub fn profile_for(dir: &Path) -> Res<Option<isoloom_spec::profile::Profile>> {
    let Some(name) = PROFILE.get().and_then(Option::as_deref) else {
        return Ok(None);
    };
    let p = isoloom_spec::profile::load(dir, name).map_err(|e| Failure::new(ErrorCode::Profile, e))?;
    INPUTS.get_or_init(|| {
        p.inputs
            .iter()
            .filter(|(k, _)| std::env::var_os(k).is_none())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    });
    Ok(Some(p))
}

/// The variables added to every tool's environment (`Command::envs`): the profile's inputs, so
/// each target's tool sees them as it sees the user's own. Empty without a profile.
pub fn child_env() -> &'static [(String, String)] {
    INPUTS.get().map_or(&[], Vec::as_slice)
}

/// An input's value as the tools see it: this process's environment, else the profile's.
pub fn input(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .or_else(|| child_env().iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))
}

/// The spec in `dir`, with the profile asked for.
pub fn load_spec(dir: &Path) -> Res<isoloom_spec::Spec> {
    let profile = profile_for(dir)?;
    Ok(isoloom_spec::load_profiled(dir, profile.as_ref(), &[])?)
}

/// The target to use: the profile's when none is asked for.
pub fn pinned(dir: &Path, asked: Option<&str>) -> Res<Option<String>> {
    Ok(match profile_for(dir)? {
        Some(p) => p.pin(asked)?,
        None => asked.map(str::to_string),
    })
}

/// The spec with `-s` overrides and the image table applied, and the defaults in effect.
pub fn load_settings(dir: &Path, images: Option<&Path>, sets: &[String]) -> Res<(isoloom_spec::Spec, isoloom_spec::defaults::Resolved)> {
    let profile = profile_for(dir)?;
    let spec = isoloom_spec::load_profiled(dir, profile.as_ref(), sets)?;
    let settings = isoloom_spec::defaults::load_profiled(dir, images, profile.as_ref(), sets)?;
    let spec = settings.defaults.images.apply(&spec);
    Ok((spec, settings))
}

/// Prints the spec's problems and returns true when it has any. Generators assume a validated
/// spec (they `expect` valid CIDRs and declared networks): a spec that parses but is invalid
/// must fail with the field and reason, not a panic.
pub fn report_invalid(spec: &isoloom_spec::Spec) -> bool {
    let problems = isoloom_spec::validate(spec);
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
pub fn require_valid(spec: &isoloom_spec::Spec) -> Res<()> {
    let problems = isoloom_spec::validate(spec);
    if !problems.is_empty() {
        if !crate::output::json() {
            for p in &problems {
                eprintln!("✗ {p}");
            }
        }
        return Err(Failure::invalid(&problems).into());
    }
    Ok(())
}

/// The spec as instance `n` when one is named: its names, blocks and ports are the instance's.
pub fn as_instance(spec: isoloom_spec::Spec, instance: Option<u8>) -> Res<isoloom_spec::Spec> {
    Ok(match instance {
        Some(n) => isoloom_spec::instance::apply(&spec, n)?,
        None => spec,
    })
}

/// The spec, as the instance when one is named (its names and ports are the instance's).
pub fn load_as(dir: &Path, instance: Option<u8>) -> Res<isoloom_spec::Spec> {
    as_instance(load_spec(dir)?, instance)
}

/// The spec with `-s` overrides, validated, as the instance when one is named.
pub fn load_valid(dir: &Path, instance: Option<u8>, sets: &[String]) -> Res<isoloom_spec::Spec> {
    let (spec, _) = load_settings(dir, None, sets)?;
    require_valid(&spec)?;
    as_instance(spec, instance)
}

/// The resolved snapshot of a project (as an instance when asked), after validation.
pub fn snapshot_of(dir: &Path, instance: Option<u8>, sets: &[String]) -> Res<serde_json::Value> {
    let spec = load_valid(dir, instance, sets)?;
    Ok(isoloom_targets::resolved::resolve_with(&spec, instance))
}

/// A project folder as an absolute path (the registry and the tools' working directories need one).
pub fn abs(dir: &Path) -> Res<PathBuf> {
    std::fs::canonicalize(dir).map_err(|e| Failure::new(ErrorCode::Io, format!("{}: {e}", dir.display())).into())
}
