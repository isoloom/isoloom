//! The spec of a project as a command sees it: with the profile asked for, the command's `-s`
//! overrides and the image table, checked, and as the instance when one is named.

use std::path::Path;

use isoloom_spec::defaults::Resolved;
use isoloom_targets::Lab;

use crate::error::{CliError, Result};

/// The profile a command runs with: `--profile`, else `ISOLOOM_PROFILE` when it isn't empty.
/// Worked out once in `main` and handed to every command that reads the spec.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Profile(Option<String>);

impl Profile {
    /// The profile asked for on the command line, else the one in the environment variable.
    pub(crate) fn new(asked: Option<String>, from_env: Option<String>) -> Self {
        Self(asked.or_else(|| from_env.filter(|s| !s.is_empty())))
    }

    /// The profile, read from `dir`; none when none was asked for.
    pub(crate) fn load(&self, dir: &Path) -> Result<Option<isoloom_spec::profile::Profile>> {
        let Some(name) = self.0.as_deref() else { return Ok(None) };
        isoloom_spec::profile::load(dir, name).map(Some).map_err(CliError::Profile)
    }
}

/// The spec file's name in `dir` (`isoloom.yml`), as problems name it.
pub(crate) fn file_name(dir: &Path) -> String {
    isoloom_spec::find(dir)
        .ok()
        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
        .unwrap_or_else(|| isoloom_spec::SPEC_FILE.to_owned())
}

/// The lab in `dir`, with the profile.
pub(crate) fn lab(profile: &Profile, dir: &Path) -> Result<Lab> {
    let profile = profile.load(dir)?;
    Ok(Lab::new(isoloom_spec::load_profiled(dir, profile.as_ref(), &[])?))
}

/// The target to use: the profile's when none is asked for.
pub(crate) fn pinned(profile: &Profile, dir: &Path, asked: Option<&str>) -> Result<Option<String>> {
    match profile.load(dir)? {
        Some(p) => p.pin(asked).map_err(CliError::Profile),
        None => Ok(asked.map(str::to_owned)),
    }
}

/// The host this command runs on: its environment, read once here, at the edge.
pub(crate) fn host() -> isoloom_spec::host::Host {
    isoloom_spec::host::Host::new(std::env::vars_os())
}

/// Where a project's defaults come from, on this host.
pub(crate) fn defaults_of<'a>(
    project: &'a Path,
    images_file: Option<&'a Path>,
    profile: Option<&'a isoloom_spec::profile::Profile>,
    sets: &'a [String],
) -> isoloom_spec::defaults::Sources<'a> {
    isoloom_spec::defaults::Sources {
        project,
        images_file,
        profile,
        sets,
        host: host(),
    }
}

/// The lab with `-s` overrides and the image table applied, and the defaults in effect.
pub(crate) fn with_settings(profile: &Profile, dir: &Path, images: Option<&Path>, sets: &[String]) -> Result<(Lab, Resolved)> {
    let profile = profile.load(dir)?;
    let spec = isoloom_spec::load_profiled(dir, profile.as_ref(), sets)?;
    let settings = isoloom_spec::defaults::load(&defaults_of(dir, images, profile.as_ref(), sets)).map_err(CliError::Defaults)?;
    let lab = Lab::new(spec).with_images(settings.defaults.images.clone());
    Ok((lab, settings))
}

/// The lab, when it has no problem: generators assume a checked spec, so a spec that parses but
/// is wrong stops with its problems, never a panic.
pub(crate) fn checked(lab: Lab, dir: &Path) -> Result<Lab> {
    let problems = lab.problems();
    if problems.is_empty() {
        Ok(lab)
    } else {
        Err(CliError::InvalidSpec {
            file: file_name(dir),
            problems,
        })
    }
}

/// The lab with `-s` overrides, checked, and checked that instance `instance` can be made of it
/// when one is asked.
pub(crate) fn valid(profile: &Profile, dir: &Path, instance: Option<u8>, sets: &[String]) -> Result<Lab> {
    let (lab, _) = with_settings(profile, dir, None, sets)?;
    let lab = checked(lab, dir)?;
    if let Some(number) = instance {
        isoloom_spec::instance::apply(&lab.spec(), number).map_err(|error| CliError::Instance { number, error })?;
    }
    Ok(lab)
}

/// The resolved snapshot of a project (as an instance when asked), once it is valid.
pub(crate) fn snapshot(profile: &Profile, dir: &Path, instance: Option<u8>, sets: &[String]) -> Result<serde_json::Value> {
    let lab = valid(profile, dir, instance, sets)?;
    Ok(isoloom_targets::resolved::resolve_with(&lab, instance))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_profile_wins_and_an_empty_variable_is_none() {
        assert_eq!(Profile::new(Some("a".into()), Some("b".into())), Profile(Some("a".to_owned())));
        assert_eq!(Profile::new(None, Some("b".into())), Profile(Some("b".to_owned())));
        assert_eq!(Profile::new(None, Some(String::new())), Profile(None));
    }

    #[test]
    fn no_profile_asked_loads_none_and_pins_what_was_asked() -> Result<()> {
        let none = Profile::default();
        assert!(none.load(Path::new("nowhere"))?.is_none());
        assert_eq!(pinned(&none, Path::new("nowhere"), Some("docker"))?, Some("docker".to_owned()));
        Ok(())
    }
}
