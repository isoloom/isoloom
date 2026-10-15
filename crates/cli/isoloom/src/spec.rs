//! The spec of a project as a command sees it: with the profile asked for, the command's `-s`
//! overrides and the image table, validated, and as the instance when one is named.

use std::path::Path;

use crate::Res;
use crate::error::CliError;

/// The profile a command runs with: `--profile`, else `ISOLOOM_PROFILE` when it isn't empty.
/// Worked out once in `main` and handed to every command that reads the spec.
#[derive(Debug, Clone, Default)]
pub struct Profile(Option<String>);

impl Profile {
    /// The profile asked for on the command line, else the one in the environment variable.
    pub fn new(asked: Option<String>, from_env: Option<String>) -> Self {
        Self(asked.or_else(|| from_env.filter(|s| !s.is_empty())))
    }

    /// The profile, read from `dir`; none when none was asked for.
    pub fn load(&self, dir: &Path) -> Res<Option<isoloom_spec::profile::Profile>> {
        let Some(name) = self.0.as_deref() else { return Ok(None) };
        let profile = isoloom_spec::profile::load(dir, name).map_err(CliError::Profile)?;
        Ok(Some(profile))
    }
}

/// The spec in `dir`, with the profile.
pub fn load_spec(profile: &Profile, dir: &Path) -> Res<isoloom_spec::Spec> {
    let profile = profile.load(dir)?;
    Ok(isoloom_spec::load_profiled(dir, profile.as_ref(), &[])?)
}

/// The target to use: the profile's when none is asked for.
pub fn pinned(profile: &Profile, dir: &Path, asked: Option<&str>) -> Res<Option<String>> {
    Ok(match profile.load(dir)? {
        Some(p) => p.pin(asked)?,
        None => asked.map(str::to_string),
    })
}

/// The spec with `-s` overrides and the image table applied, and the defaults in effect.
pub fn load_settings(profile: &Profile, dir: &Path, images: Option<&Path>, sets: &[String]) -> Res<(isoloom_spec::Spec, isoloom_spec::defaults::Resolved)> {
    let profile = profile.load(dir)?;
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

/// The spec with `-s` overrides, validated (its problems printed, then an error), as the
/// instance when one is named.
pub fn load_valid(profile: &Profile, dir: &Path, instance: Option<u8>, sets: &[String]) -> Res<isoloom_spec::Spec> {
    let (spec, _) = load_settings(profile, dir, None, sets)?;
    let problems = isoloom_spec::validate(&spec);
    if !problems.is_empty() {
        for p in &problems {
            eprintln!("✗ {p}");
        }
        return Err(CliError::SpecInvalid.into());
    }
    Ok(match instance {
        Some(n) => isoloom_spec::instance::apply(&spec, n)?,
        None => spec,
    })
}

/// The resolved snapshot of a project (as an instance when asked), after validation.
pub fn snapshot_of(profile: &Profile, dir: &Path, instance: Option<u8>, sets: &[String]) -> Res<serde_json::Value> {
    let spec = load_valid(profile, dir, instance, sets)?;
    Ok(isoloom_targets::resolved::resolve_with(&spec, instance))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_profile_wins_and_an_empty_variable_is_none() {
        assert_eq!(Profile::new(Some("a".into()), Some("b".into())).0.as_deref(), Some("a"));
        assert_eq!(Profile::new(None, Some("b".into())).0.as_deref(), Some("b"));
        assert_eq!(Profile::new(None, Some(String::new())).0, None);
    }
}
