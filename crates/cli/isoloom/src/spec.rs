//! The spec of a project as a command sees it: with the profile asked for, the command's `-s`
//! overrides and the image table, checked, and as the instance when one is named.

use std::path::{Path, PathBuf};

use isoloom_spec::defaults::Resolved;
use isoloom_targets::Lab;

use crate::error::{CliError, Result};

// What every command that reads the lab reads it with besides the spec and the profile, so each
// judges the lab `generate` writes; flattened into its arguments (no doc comment: clap would print
// it in `--help`).
#[derive(Debug, Clone, Default, clap::Args)]
pub(crate) struct Settings {
    /// Your own image table (YAML): OS images, and the access machine when the spec leaves
    /// it to the runner. See <https://www.isoloom.com/en/docs/configuration#the-image-table>
    #[arg(long, value_name = "FILE")]
    pub(crate) images: Option<PathBuf>,
    /// Override a value for this command: `-s machines.web.os=ubuntu@24.04` on the spec,
    /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    pub(crate) sets: Vec<String>,
    /// Allow the environment to reach this computer this way: `host-folders` (a folder outside
    /// the project), `writable-host-folders`, `docker-socket`, `privileged` (a privileged
    /// container), `devices`, `boot-files` (a VM's kernel, firmware or TFTP folder on the host),
    /// `host-network` (a bridged network). None by default: a spec that asks for one is refused.
    /// Repeatable.
    #[arg(long = "allow-host", value_name = "ACCESS", value_parser = host_access)]
    pub(crate) allow_host: Vec<isoloom_spec::host_access::HostAccess>,
}

/// An access `--allow-host` names.
fn host_access(id: &str) -> std::result::Result<isoloom_spec::host_access::HostAccess, String> {
    use isoloom_spec::host_access::HostAccess;
    HostAccess::parse(id).ok_or_else(|| {
        let known: Vec<&str> = HostAccess::ALL.iter().map(|a| a.id()).collect();
        format!("`{id}` isn't an access: one of {}", known.join(", "))
    })
}

/// The profile a command runs with: `--profile`, else `ISOLOOM_PROFILE` when it isn't empty.
/// Worked out once in `main` and handed to every command that reads the spec.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Profile(Option<String>);

/// The environment variable naming the profile when `--profile` doesn't.
pub(crate) const PROFILE_VARIABLE: &str = "ISOLOOM_PROFILE";

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

/// Vagrant's own default provider on this host (`VAGRANT_DEFAULT_PROVIDER`, when set and not
/// empty): read here, at the edge, with the rest of the environment.
pub(crate) fn vagrant_default_provider() -> Option<String> {
    std::env::var("VAGRANT_DEFAULT_PROVIDER").ok().filter(|v| !v.is_empty())
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

/// What a command does with a template the environment fills in that can't be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unread {
    /// It is an error, with the read error: the files would be generated without it.
    Fails,
    /// It is left out, for `isoloom validate`, whose file checks report it as a problem.
    Skipped,
}

/// The environment with `-s` overrides, the image table, the clouds' regions and the defaults' tags
/// applied, and the defaults in effect: the environment every command reads, as `generate` writes it. A
/// template that can't be read is an error.
pub(crate) fn with_settings(profile: &Profile, dir: &Path, given: &Settings) -> Result<(Lab, Resolved)> {
    let profile = profile.load(dir)?;
    settled(profile.as_ref(), dir, given, Unread::Fails)
}

/// [`with_settings`] with `profile` already loaded (or none), and a template that can't be read
/// handled as `unread` says: the one pipeline every command reads the environment through, `isoloom
/// profiles` included.
pub(crate) fn settled(profile: Option<&isoloom_spec::profile::Profile>, dir: &Path, given: &Settings, unread: Unread) -> Result<(Lab, Resolved)> {
    let sets = &given.sets;
    let mut spec = isoloom_spec::load_profiled(dir, profile, sets)?;
    let settings = isoloom_spec::defaults::load(&defaults_of(dir, given.images.as_deref(), profile, sets)).map_err(CliError::Defaults)?;
    spec.tags = settings.tags_under(&spec.tags).map_err(CliError::Defaults)?;
    let lab = Lab::new(spec)
        .with_images(settings.defaults.images.clone())
        .with_regions(settings.defaults.regions())
        .with_host_access(isoloom_spec::host_access::Grants::of(given.allow_host.iter().copied()));
    let templates = templates(&lab, dir, unread)?;
    let lab = lab.with_templates(templates);
    let dockerfiles = dockerfiles(&lab, dir);
    let index = bases_index(dir)?;
    let bases = isoloom_targets::dockerfile::Sources::of(&index, |p| read_inside(dir, p).ok());
    Ok((lab.with_dockerfiles(dockerfiles).with_bases(bases), settings))
}

/// The texts of the Dockerfiles `lab`'s builds name, read from `dir`, for their machines' VM
/// form. One that can't be read is left out: its machine's VM form is refused saying so, and
/// its container form reads it as the build does.
fn dockerfiles(lab: &Lab, dir: &Path) -> std::collections::BTreeMap<isoloom_spec::syntax::ProjectPath, String> {
    lab.dockerfile_paths()
        .into_iter()
        .filter_map(|p| read_inside(dir, p.as_str()).ok().map(|text| (p, text)))
        .collect()
}

/// The project's vendored base images (`vendor/bases/index.yml`, which `isoloom bases fetch`
/// writes): empty when it has none.
pub(crate) fn bases_index(dir: &Path) -> Result<isoloom_targets::dockerfile::Index> {
    let path = dir.join(isoloom_targets::dockerfile::sources::INDEX);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_yaml_ng::from_str(&text).map_err(|e| CliError::Read {
            path,
            source: std::io::Error::other(e),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(isoloom_targets::dockerfile::Index::new()),
        Err(source) => Err(CliError::Read { path, source }),
    }
}

/// The texts of the project files `lab`'s steps fill in as templates, read from `dir`. One that
/// can't be read fails with the read error, or is left out when `unread` says so.
fn templates(lab: &Lab, dir: &Path, unread: Unread) -> Result<std::collections::BTreeMap<isoloom_spec::syntax::ProjectPath, String>> {
    let mut out = std::collections::BTreeMap::new();
    for p in lab.template_paths() {
        match read_inside(dir, p.as_str()) {
            Ok(text) => {
                out.insert(p, text);
            }
            // A path out of the project is never read: the spec's checks refuse it.
            Err(Unreadable::Escapes) => {}
            Err(_) if unread == Unread::Skipped => {}
            Err(Unreadable::Read(source)) => {
                return Err(CliError::Read {
                    path: dir.join(p.as_str()),
                    source,
                });
            }
        }
    }
    Ok(out)
}

/// Why a project file wasn't read.
#[derive(Debug)]
enum Unreadable {
    /// It isn't a plain path of the project, or leads out of it through a symbolic link.
    Escapes,
    /// It can't be read (it isn't there, among others).
    Read(std::io::Error),
}

/// The text of project file `path` in `dir`, read only when it is inside the project once its
/// links are followed ([`isoloom_spec::project_path::inside`]): a file the project links to
/// elsewhere on this computer is never read into the generated files.
fn read_inside(dir: &Path, path: &str) -> std::result::Result<String, Unreadable> {
    use isoloom_spec::project_path::{Escape, inside};
    match inside(dir, path) {
        Ok(real) => std::fs::read_to_string(real).map_err(Unreadable::Read),
        // The system's own words for what isn't there, read without opening it.
        Err(Escape::Missing) => Err(Unreadable::Read(
            std::fs::metadata(dir.join(path))
                .err()
                .unwrap_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound)),
        )),
        Err(_) => Err(Unreadable::Escapes),
    }
}

/// The environment, when it has no problem, the project files it names included (each there, and
/// inside the project once its links are followed): generators assume a checked spec, so a spec
/// that parses but is wrong stops with its problems, never a panic, and nothing is generated from
/// a file out of the project.
pub(crate) fn checked(lab: Lab, dir: &Path) -> Result<Lab> {
    let problems = lab.problems_with_files(dir);
    if problems.is_empty() {
        Ok(lab)
    } else {
        Err(CliError::InvalidSpec {
            file: file_name(dir),
            problems,
        })
    }
}

/// The environment read with `settings` ([`with_settings`]), checked, and checked that instance
/// `instance` can be made of it when one is asked; with the defaults in effect.
pub(crate) fn valid(profile: &Profile, dir: &Path, instance: Option<u8>, settings: &Settings) -> Result<(Lab, Resolved)> {
    let (lab, resolved) = with_settings(profile, dir, settings)?;
    let lab = checked(lab, dir)?;
    if let Some(number) = instance {
        isoloom_spec::instance::apply(&lab.spec(), number).map_err(|error| CliError::Instance { number, error })?;
    }
    Ok((lab, resolved))
}

/// The resolved snapshot of the environment [`valid`] reads (as an instance when asked).
pub(crate) fn snapshot(lab: &Lab, instance: Option<u8>) -> Result<serde_json::Value> {
    Ok(isoloom_targets::resolved::resolve_with(lab, instance)?)
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
