//! A lab's files for a target: lowered for the target, planned, rendered by the target's
//! backend, with its run manifest and the resolved snapshot beside them.

use std::path::Path;

use isoloom_plan::{Environment, Lab};
use isoloom_render::{GeneratedFile, OUTPUT_DIR};
use isoloom_spec::model::{Spec, Target};

use crate::registry::backend;
use crate::{Error, GenerateError, Result, resolved};

/// The files for one target, the resolved snapshot among them.
///
/// # Errors
///
/// [`Error::Refused`]: the target isn't possible for the lab, has no backend, or its backend
/// refuses the lab.
pub fn generate(lab: &Lab, target: Target) -> Result<Vec<GeneratedFile>> {
    Ok(with_snapshot(lab, target)?)
}

/// A target's own files, then the resolved snapshot, which goes with every target (the same
/// file each time).
fn with_snapshot(lab: &Lab, target: Target) -> std::result::Result<Vec<GeneratedFile>, GenerateError> {
    let mut files = target_files(lab, target, None)?;
    files.push(resolved::file(lab));
    Ok(files)
}

/// The files for one target as instance `n` of the lab (see [`isoloom_spec::instance`]): names
/// suffixed, Docker blocks and published ports moved, everything under `.isoloom-<n>/`.
///
/// # Errors
///
/// As [`generate`], and [`Error::Refused`] when the instance can't be made.
pub fn generate_instance(lab: &Lab, target: Target, n: u8) -> Result<Vec<GeneratedFile>> {
    let mut files = target_files(lab, target, Some(n))?;
    files.push(resolved::file_with(lab, Some(n)));
    Ok(files
        .into_iter()
        .map(|f| {
            let (path, contents) = isoloom_spec::instance::relocate(&f.path, &f.contents, n);
            GeneratedFile { path, contents }
        })
        .collect())
}

/// Every target the lab allows: the files (each once: `docker-vm` runs the `docker` target's
/// Compose file), and why each target left out wasn't generated.
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
                // The resolved snapshot is the same for every target (it asks each target
                // whether it refuses the lab): written once, after the first target's files.
                if first {
                    files.push(resolved::file(lab));
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
    let env = Environment::build(&spec, target, instance);
    let mut files = backend.render(&env)?;
    if let Some(manifest) = backend.run_manifest(&env)? {
        files.push(run_manifest(lab, instance, target, manifest)?);
    }
    Ok(files)
}

/// A backend's run manifest as the file it is written to (`.isoloom/<target>/run.json`), its
/// message filled from the resolved snapshot, checked across its fields.
fn run_manifest(
    lab: &Lab,
    instance: Option<u8>,
    target: Target,
    mut manifest: isoloom_manifest::Manifest,
) -> std::result::Result<GeneratedFile, GenerateError> {
    let refused = |e: isoloom_manifest::Error| GenerateError::Unsupported {
        target,
        what: format!("the run manifest: {e}"),
    };
    manifest.lab.message = resolved::manifest_message(lab, instance, target);
    manifest.validate().map_err(refused)?;
    Ok(GeneratedFile {
        path: format!("{OUTPUT_DIR}/{}/run.json", target.id()),
        contents: isoloom_manifest::to_json(&manifest).map_err(refused)?,
    })
}
