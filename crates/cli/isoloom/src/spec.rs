//! The spec of a project as a command sees it: with the profile asked for, the command's `-s`
//! overrides and the image table, validated, and as the instance when one is named.

use std::path::Path;

use isoloom_targets::Lab;

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

/// The spec file's name in `dir` (`isoloom.yml`), as problems name it.
fn file_name(dir: &Path) -> String {
    isoloom_spec::find(dir)
        .ok()
        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
        .unwrap_or_else(|| isoloom_spec::SPEC_FILE.to_owned())
}

/// Prints problems as an author reads them, each under a `✗`.
fn print_problems(file: &str, problems: &[isoloom_spec::syntax::Problem]) {
    for p in problems {
        eprintln!("✗ {}", p.render(file));
    }
}

/// A spec that didn't load: its problems printed, then the command's error; any other error as
/// it is.
fn loaded(dir: &Path, r: Result<isoloom_spec::Spec, isoloom_spec::LoadError>) -> Res<isoloom_spec::Spec> {
    match r {
        Ok(spec) => Ok(spec),
        Err(isoloom_spec::LoadError::Invalid { problems, .. }) => {
            print_problems(&file_name(dir), &problems);
            Err(CliError::SpecInvalid.into())
        }
        Err(e) => Err(e.into()),
    }
}

/// The lab in `dir`, with the profile.
pub fn load_lab(profile: &Profile, dir: &Path) -> Res<Lab> {
    let profile = profile.load(dir)?;
    Ok(Lab::new(loaded(dir, isoloom_spec::load_profiled(dir, profile.as_ref(), &[]))?))
}

/// The target to use: the profile's when none is asked for.
pub fn pinned(profile: &Profile, dir: &Path, asked: Option<&str>) -> Res<Option<String>> {
    Ok(match profile.load(dir)? {
        Some(p) => p.pin(asked)?,
        None => asked.map(str::to_string),
    })
}

/// The lab with `-s` overrides and the image table applied, and the defaults in effect.
pub fn load_settings(profile: &Profile, dir: &Path, images: Option<&Path>, sets: &[String]) -> Res<(Lab, isoloom_spec::defaults::Resolved)> {
    let profile = profile.load(dir)?;
    let spec = loaded(dir, isoloom_spec::load_profiled(dir, profile.as_ref(), sets))?;
    let settings = isoloom_spec::defaults::load_profiled(dir, images, profile.as_ref(), sets)?;
    let lab = Lab::new(spec).with_images(settings.defaults.images.clone());
    Ok((lab, settings))
}

/// Prints the lab's problems and returns true when it has any. Generators assume a validated
/// spec: a spec that parses but is invalid must fail with the field and reason, not a panic.
pub fn report_invalid(dir: &Path, lab: &Lab) -> bool {
    let problems = lab.problems();
    print_problems(&file_name(dir), &problems);
    if !problems.is_empty() {
        eprintln!("fix the spec first (`isoloom validate`)");
    }
    !problems.is_empty()
}

/// The lab with `-s` overrides, validated (its problems printed, then an error), checked that
/// instance `instance` can be made when one is asked.
pub fn load_valid(profile: &Profile, dir: &Path, instance: Option<u8>, sets: &[String]) -> Res<Lab> {
    let (lab, _) = load_settings(profile, dir, None, sets)?;
    let problems = lab.problems();
    if !problems.is_empty() {
        print_problems(&file_name(dir), &problems);
        return Err(CliError::SpecInvalid.into());
    }
    if let Some(n) = instance {
        isoloom_spec::instance::apply(&lab.spec(), n)?;
    }
    Ok(lab)
}

/// The resolved snapshot of a project (as an instance when asked), after validation.
pub fn snapshot_of(profile: &Profile, dir: &Path, instance: Option<u8>, sets: &[String]) -> Res<serde_json::Value> {
    let lab = load_valid(profile, dir, instance, sets)?;
    Ok(isoloom_targets::resolved::resolve_with(&lab, instance))
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
