//! An environment's files for a target: lowered for the target, planned, rendered by the target's
//! backend, with the resolved snapshot beside them.

use std::path::Path;

use isoloom_plan::{Environment, Lab};
use isoloom_render::GeneratedFile;
use isoloom_spec::contract::OUTPUT_DIR;
use isoloom_spec::model::{Spec, Target};

use crate::registry::backend;
use crate::{Error, GenerateError, Result, gitignore, resolved};

/// The files for one target, the resolved snapshot among them.
///
/// # Errors
///
/// [`Error::Generate`]: the target isn't possible for the environment, or its backend refuses the environment or
/// fails.
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

/// The files for one target as instance `n` of the environment (see [`isoloom_spec::instance`]): names
/// suffixed, Docker blocks and published ports moved, everything under `.isoloom-<n>/`.
///
/// # Errors
///
/// As [`generate`], and [`Error::Generate`] with [`GenerateError::Instance`] when the instance
/// can't be made.
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

/// Every target the environment allows: the files (each once: `cloud-docker` runs the `docker`
/// target's Compose file), and why each target left out wasn't generated. A caller that writes or checks
/// the files fails on any that isn't a refusal ([`GenerateError::is_refusal`]): a failed backend
/// leaves its target's files out, which would otherwise read as a target the environment can't use.
#[must_use]
pub fn generate_all(lab: &Lab) -> (Vec<GeneratedFile>, Vec<GenerateError>) {
    let mut files: Vec<GeneratedFile> = Vec::new();
    let mut skipped = Vec::new();
    for target in lab.possible() {
        let first = files.is_empty();
        match target_files(lab, target, None).and_then(|own| unwritten(&files, target, own)) {
            Ok(new) => {
                files.extend(new);
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

/// Of `target`'s files (`own`), the ones the earlier targets' (`files`) don't hold yet, each path
/// once (a file an earlier target wrote the same is theirs). A file at an earlier target's path
/// with other contents would replace it, a bug of one of the two backends: an error.
fn unwritten(files: &[GeneratedFile], target: Target, own: Vec<GeneratedFile>) -> std::result::Result<Vec<GeneratedFile>, GenerateError> {
    if let Some(clash) = own.iter().find(|f| files.iter().any(|x| x.path == f.path && x.contents != f.contents)) {
        return Err(GenerateError::Internal {
            target,
            what: format!("`{}` is an earlier target's file, with other contents", clash.path),
            source: std::sync::Arc::new(WrittenTwice),
        });
    }
    let mut new: Vec<GeneratedFile> = Vec::new();
    for file in own {
        if !files.iter().chain(&new).any(|x| x.path == file.path) {
            new.push(file);
        }
    }
    Ok(new)
}

/// The project's own files the generated ones copy, read from the project folder `dir`: when
/// `files` hold the Kubernetes target's (its folder under `.isoloom/`), the scripts its plan of
/// `lab` runs, which its Helm chart and kustomization read.
///
/// # Errors
///
/// [`Error::ProjectCopies`] when one can't be read.
pub fn project_copies(lab: &Lab, files: &[GeneratedFile], dir: &Path) -> Result<Vec<GeneratedFile>> {
    let target = Target::Kubernetes;
    let folder = format!("{OUTPUT_DIR}/{}/", target.id());
    if !files.iter().any(|f| f.path.starts_with(&folder)) {
        return Ok(Vec::new());
    }
    let env = planned(lab, &lowered(lab, target, None)?, target, None)?;
    isoloom_kubernetes::project_copies(&env, dir).map_err(Error::ProjectCopies)
}

/// `spec`, the environment as `target` runs it (as instance `instance`), planned. An environment that lowered
/// clean always plans: one that doesn't is a failure, not a refusal.
pub(crate) fn planned(lab: &Lab, spec: &Spec, target: Target, instance: Option<u8>) -> std::result::Result<Environment, GenerateError> {
    lab.plan(spec, target, instance).map_err(|e| GenerateError::Internal {
        target,
        what: "the lab couldn't be planned".to_owned(),
        source: std::sync::Arc::new(e),
    })
}

/// The environment as `target` runs it: lowered for it, as instance `n` when one is asked.
fn lowered(lab: &Lab, target: Target, instance: Option<u8>) -> std::result::Result<Spec, GenerateError> {
    let spec = lab.for_target(target);
    match instance {
        Some(n) => isoloom_spec::instance::apply(&spec, n).map_err(|source| GenerateError::Instance { target, number: n, source }),
        None => Ok(spec),
    }
}

/// A target's own files, without the snapshot (which asks every backend whether it would
/// refuse the environment, so it can't be part of what a backend renders): the plan for the target,
/// rendered by its backend.
fn target_files(lab: &Lab, target: Target, instance: Option<u8>) -> std::result::Result<Vec<GeneratedFile>, GenerateError> {
    let problems = lab.problems();
    if !problems.is_empty() {
        // The generators assume a checked spec: a spec that parses but is wrong stops here,
        // whoever calls, as the CLI stops it.
        return Err(GenerateError::Invalid {
            problems: problems.iter().map(isoloom_spec::syntax::Problem::line).collect(),
        });
    }
    if let Some(refused) = lab.refusal(target) {
        return Err(GenerateError::Unsupported { target, what: refused.line() });
    }
    if !lab.possible().contains(&target) {
        return Err(GenerateError::NotPossible(target));
    }
    let backend = backend(target);
    let spec = lowered(lab, target, instance)?;
    let env = planned(lab, &spec, target, instance)?;
    let rendered = backend.render(&env)?;
    assembled(target, rendered, isoloom_render::configure::files(&env))
}

/// A target's files: its backend's, then the configuration renderers' own (the libraries a VM's
/// set-up embeds, a container's runner and templates; once each, since a backend may have
/// written some already). Two at one path, the second of which would replace the first, is a
/// bug of the backend: an error.
fn assembled(target: Target, mut files: Vec<GeneratedFile>, configure: Vec<GeneratedFile>) -> std::result::Result<Vec<GeneratedFile>, GenerateError> {
    let own: Vec<GeneratedFile> = configure.into_iter().filter(|f| !files.iter().any(|x| x.path == f.path)).collect();
    files.extend(own);
    match written_twice(&files) {
        Some(path) => Err(GenerateError::Internal {
            target,
            what: format!("the target's files hold `{path}` twice"),
            source: std::sync::Arc::new(WrittenTwice),
        }),
        None => Ok(files),
    }
}

/// Two generated files at one path, the second of which would replace the first: two of a
/// target's, or two targets' with other contents.
#[derive(Debug, thiserror::Error)]
#[error("two generated files at one path")]
struct WrittenTwice;

/// The first path two of `files` share, if any.
fn written_twice(files: &[GeneratedFile]) -> Option<&str> {
    let mut seen = std::collections::HashSet::new();
    files.iter().map(|f| f.path.as_str()).find(|p| !seen.insert(*p))
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
    fn a_backend_file_written_twice_is_an_error() {
        let compose = GeneratedFile::new(".isoloom/docker/compose.yml", "a");
        // A configuration file the backend wrote already is the backend's, once.
        let fine = assembled(
            Target::Docker,
            vec![compose.clone()],
            vec![GeneratedFile::new(".isoloom/docker/compose.yml", "b")],
        );
        assert_eq!(fine.map(|f| f.len()).ok(), Some(1));
        let e = assembled(Target::Docker, vec![compose.clone(), compose], vec![])
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(e.contains("twice"), "{e}");
    }

    #[test]
    fn a_path_two_targets_write_is_theirs_once_or_an_error() {
        let mut files = unwritten(&[], Target::Docker, vec![GeneratedFile::new(".isoloom/docker/compose.yml", "a")]).unwrap();
        assert_eq!(files.len(), 1);
        // The same file again (cloud-docker runs the docker target's Compose file): kept once.
        let same = vec![
            GeneratedFile::new(".isoloom/docker/compose.yml", "a"),
            GeneratedFile::new(".isoloom/cloud-docker/aws/main.tf", "v"),
        ];
        files.extend(unwritten(&files, Target::CloudDocker, same).unwrap());
        assert_eq!(files.len(), 2);
        // Other contents at that path would replace it: an error, and nothing of the target added.
        let other = vec![
            GeneratedFile::new(".isoloom/kubernetes/manifest.yaml", "h"),
            GeneratedFile::new(".isoloom/docker/compose.yml", "b"),
        ];
        let e = unwritten(&files, Target::Kubernetes, other).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(e.contains("`.isoloom/docker/compose.yml` is an earlier target's file"), "{e}");
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

    #[test]
    fn a_lab_that_doesnt_plan_fails_the_generation_rather_than_being_refused() {
        let lab = Lab::parse(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  win: { networks: { lan: 10 }, os: windows-server@2022, app: { steps: [{ copy: { from: a, to: 'C:\\a', owner: admin } }] } }\n",
        )
        .unwrap();
        let failed = planned(&lab, &lab.for_target(Target::Vagrant), Target::Vagrant, None).unwrap_err();
        assert!(matches!(failed, GenerateError::Internal { target: Target::Vagrant, .. }), "{failed:?}");
        assert!(!failed.is_refusal());
        assert!(
            std::error::Error::source(&failed).is_some_and(|e| e.to_string().starts_with("machine `win`")),
            "{failed}"
        );
    }
}
