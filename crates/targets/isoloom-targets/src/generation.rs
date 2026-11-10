//! A lab's files for a target: lowered for the target, planned, rendered by the target's
//! backend, with its run manifest and the resolved snapshot beside them.

use std::path::Path;

use isoloom_plan::Lab;
use isoloom_render::backend::Placed;
use isoloom_render::{GeneratedFile, OUTPUT_DIR};
use isoloom_spec::model::{Spec, Target};

use crate::registry::backend;
use crate::{Error, GenerateError, Result, gitignore, resolved};

/// The files for one target, the resolved snapshot among them.
///
/// # Errors
///
/// [`Error::Generate`]: the target isn't possible for the lab, has no backend, or its backend
/// refuses the lab or fails.
pub fn generate(lab: &Lab, target: Target) -> Result<Vec<GeneratedFile>> {
    Ok(with_snapshot(lab, target)?)
}

/// A target's own files, then the resolved snapshot and `.isoloom/.gitignore`, which go with
/// every target (the same files each time).
fn with_snapshot(lab: &Lab, target: Target) -> std::result::Result<Vec<GeneratedFile>, GenerateError> {
    let mut files = target_files(lab, target, None)?;
    files.push(resolved::file(lab));
    files.push(gitignore::file());
    Ok(files)
}

/// The files for one target as instance `n` of the lab (see [`isoloom_spec::instance`]): names
/// suffixed, Docker blocks and published ports moved, everything under `.isoloom-<n>/`.
///
/// # Errors
///
/// As [`generate`], and [`Error::Generate`] when the instance can't be made.
pub fn generate_instance(lab: &Lab, target: Target, n: u8) -> Result<Vec<GeneratedFile>> {
    let mut files = target_files(lab, target, Some(n))?;
    files.push(resolved::file_with(lab, n)?);
    files.push(gitignore::file());
    Ok(files
        .into_iter()
        .map(|f| {
            let (path, contents) = isoloom_spec::instance::relocate(&f.path, &f.contents, n);
            GeneratedFile { path, contents }
        })
        .collect())
}

/// Every target the lab allows: the files (each once: `docker-vm` runs the `docker` target's
/// Compose file), and why each target left out wasn't generated. A caller that writes or checks
/// the files fails on any that isn't a refusal ([`GenerateError::is_refusal`]): a failed backend
/// leaves its target's files out, which would otherwise read as a target the lab can't use.
#[must_use]
pub fn generate_all(lab: &Lab) -> (Vec<GeneratedFile>, Vec<GenerateError>) {
    let mut files: Vec<GeneratedFile> = Vec::new();
    let mut skipped = Vec::new();
    for target in lab.possible() {
        match target_files(lab, target, None) {
            Ok(own) => {
                let first = files.is_empty();
                for file in own {
                    if !files.iter().any(|x| x.path == file.path) {
                        files.push(file);
                    }
                }
                // The resolved snapshot (it asks each target whether it refuses the lab) and the
                // .gitignore are the same for every target: written once, after the first
                // target's files.
                if first {
                    files.push(resolved::file(lab));
                    files.push(gitignore::file());
                }
            }
            Err(e) => skipped.push(e),
        }
    }
    (files, skipped)
}

/// The project's own files the generated ones copy (the scripts a Helm chart's pods run), read
/// from the project folder `dir`.
///
/// # Errors
///
/// [`Error::ProjectCopies`] when one can't be read.
pub fn project_copies(files: &[GeneratedFile], dir: &Path) -> Result<Vec<GeneratedFile>> {
    isoloom_kubernetes::project_copies(files, dir).map_err(Error::ProjectCopies)
}

/// The lab as `target` runs it: lowered for it, as instance `n` when one is asked.
fn lowered(lab: &Lab, target: Target, instance: Option<u8>) -> std::result::Result<Spec, GenerateError> {
    let spec = lab.for_target(target);
    match instance {
        Some(n) => isoloom_spec::instance::apply(&spec, n).map_err(|e| GenerateError::Unsupported { target, what: e.to_string() }),
        None => Ok(spec),
    }
}

/// A target's own files, without the snapshot (which asks every backend whether it would
/// refuse the lab, so it can't be part of what a backend renders): the plan for the target,
/// rendered by its backend.
fn target_files(lab: &Lab, target: Target, instance: Option<u8>) -> std::result::Result<Vec<GeneratedFile>, GenerateError> {
    if let Some(refused) = lab.refusal(target) {
        return Err(GenerateError::Unsupported { target, what: refused.line() });
    }
    if !lab.possible().contains(&target) {
        return Err(GenerateError::NotPossible(target));
    }
    let backend = backend(target).ok_or(GenerateError::NoGenerator(target))?;
    let spec = lowered(lab, target, instance)?;
    let env = lab.plan(&spec, target, instance);
    let rendered = backend.render(&env)?;
    let manifests = backend
        .run_manifests(&env)?
        .into_iter()
        .map(|placed| run_manifest(lab, instance, target, placed))
        .collect::<Result<Vec<_>, _>>()?;
    assembled(target, rendered, isoloom_render::configure::files(&env), manifests)
}

/// A target's files: its backend's, then the configuration renderers' own (the libraries a VM's
/// set-up embeds, a container's runner and templates; once each, since a backend may have
/// written some already), then its run manifests. Two at one path, the second of which would
/// replace the first, is a bug of the backend: an error.
fn assembled(
    target: Target,
    mut files: Vec<GeneratedFile>,
    configure: Vec<GeneratedFile>,
    manifests: Vec<GeneratedFile>,
) -> std::result::Result<Vec<GeneratedFile>, GenerateError> {
    let own: Vec<GeneratedFile> = configure.into_iter().filter(|f| !files.iter().any(|x| x.path == f.path)).collect();
    files.extend(own);
    files.extend(manifests);
    match written_twice(&files) {
        Some(path) => Err(GenerateError::Internal {
            target,
            what: format!("the target's files hold `{path}` twice"),
            source: std::sync::Arc::new(WrittenTwice),
        }),
        None => Ok(files),
    }
}

/// Two of a target's files at one path, the second of which would replace the first.
#[derive(Debug, thiserror::Error)]
#[error("two generated files at one path")]
struct WrittenTwice;

/// The first path two of `files` share, if any.
fn written_twice(files: &[GeneratedFile]) -> Option<&str> {
    let mut seen = std::collections::HashSet::new();
    files.iter().map(|f| f.path.as_str()).find(|p| !seen.insert(*p))
}

/// A backend's run manifest as the file it is written to (`.isoloom/<target>/[<sub>/]run.json`),
/// its message filled from the resolved snapshot, checked across its fields.
fn run_manifest(lab: &Lab, instance: Option<u8>, target: Target, placed: Placed) -> std::result::Result<GeneratedFile, GenerateError> {
    let refused = |e: isoloom_manifest::Error| GenerateError::Internal {
        target,
        what: "the run manifest refused a value of this backend".to_owned(),
        source: std::sync::Arc::new(e),
    };
    let mut manifest = placed.manifest;
    manifest.environment.message = resolved::manifest_message(lab, instance, target);
    manifest.validate().map_err(refused)?;
    let folder = placed.sub.map_or_else(|| target.id().to_owned(), |sub| format!("{}/{sub}", target.id()));
    Ok(GeneratedFile {
        path: format!("{OUTPUT_DIR}/{folder}/run.json"),
        contents: isoloom_manifest::to_json(&manifest).map_err(refused)?,
    })
}

/// Each cloud's id and the region its modules default to when the person's defaults set none
/// (`isoloom defaults --system`), in the order the modules are written.
#[must_use]
pub fn default_regions() -> Vec<(&'static str, &'static str)> {
    isoloom_terraform::Cloud::ALL.iter().map(|c| (c.id(), c.default_region())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_manifest_or_a_backend_file_written_twice_is_an_error() {
        let compose = GeneratedFile::new(".isoloom/docker/compose.yml", "a");
        let manifest = GeneratedFile::new(".isoloom/docker/run.json", "{}");
        // A configuration file the backend wrote already is the backend's, once.
        let fine = assembled(
            Target::Docker,
            vec![compose.clone()],
            vec![GeneratedFile::new(".isoloom/docker/compose.yml", "b")],
            vec![manifest.clone()],
        );
        assert_eq!(fine.map(|f| f.len()).ok(), Some(2));
        // A run manifest at a path the backend wrote is caught, as two of the backend's are.
        for (rendered, manifests) in [
            (
                vec![compose.clone(), GeneratedFile::new(".isoloom/docker/run.json", "x")],
                vec![manifest.clone()],
            ),
            (vec![compose.clone(), compose.clone()], vec![]),
        ] {
            let e = assembled(Target::Docker, rendered, vec![], manifests)
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default();
            assert!(e.contains("twice"), "{e}");
        }
    }

    fn at(path: &str) -> GeneratedFile {
        GeneratedFile {
            path: path.to_owned(),
            contents: String::new(),
        }
    }

    #[test]
    fn a_path_two_files_share_is_found() {
        assert_eq!(written_twice(&[at("a"), at("b"), at("a")]), Some("a"));
        assert_eq!(written_twice(&[at("a"), at("b")]), None);
    }
}
