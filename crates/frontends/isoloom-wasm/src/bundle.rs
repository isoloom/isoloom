//! The zips the user downloads: the whole project, or one target's files.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;

use crate::plan::{compiled, planned};
use crate::{Error, File, Result};

/// The zip the user downloads: `isoloom.yml` at the root, the project's own files where it points,
/// and each generated file at its path.
///
/// # Errors
///
/// [`Error`]: the topology is malformed or doesn't compile, Isoloom finds problems in the
/// spec, no target generates any file for the project ([`Error::NothingGenerated`]), or the zip
/// can't be written.
pub(crate) fn bundle_bytes(topology_json: &str) -> Result<Vec<u8>> {
    bundle_filtered(topology_json, None)
}

/// One target's zip: `isoloom.yml` and only that target's folder (`docker`, `hybrid`,
/// `cloud-docker/aws`...), with every one of the project's own files: which ones a target's files
/// use is Isoloom's to know, and a file left out would break the project where one kept that
/// isn't run costs a few lines.
///
/// # Errors
///
/// [`Error`], as [`bundle_bytes`], or [`Error::NoFiles`] when the project generates
/// nothing for `target_dir`.
pub(crate) fn bundle_target_bytes(topology_json: &str, target_dir: &str) -> Result<Vec<u8>> {
    bundle_filtered(topology_json, Some(target_dir))
}

fn bundle_filtered(topology_json: &str, target_dir: Option<&str>) -> Result<Vec<u8>> {
    let (topo, compiled) = compiled(topology_json)?;
    let (yaml, spec, own) = compiled.into_parts();
    let own: Vec<File> = own.into_iter().map(File::from).collect();
    let plan = planned(&topo, yaml.clone(), spec);
    if !plan.problems.is_empty() {
        return Err(Error::Problems(plan.problems));
    }
    let files = target_files(plan.files, target_dir)?;
    let spec_file = File {
        path: isoloom_spec::SPEC_FILE.into(),
        contents: yaml,
    };
    zip_of(std::iter::once(&spec_file).chain(&own).chain(&files))
}

/// The generated files of the target in `target_dir` (`docker`, `cloud-docker/aws`…), or all.
fn target_files(files: Vec<File>, target_dir: Option<&str>) -> Result<Vec<File>> {
    let Some(dir) = target_dir else {
        return if files.is_empty() { Err(Error::NothingGenerated) } else { Ok(files) };
    };
    let prefix = format!("{}/{}/", isoloom_spec::instance::output_dir(None), dir.trim_matches('/'));
    let files: Vec<File> = files.into_iter().filter(|f| f.path.starts_with(&prefix)).collect();
    if files.is_empty() {
        return Err(Error::NoFiles(dir.to_owned()));
    }
    Ok(files)
}

/// A zip of `files`, deflated, in order.
fn zip_of<'a>(files: impl Iterator<Item = &'a File>) -> Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for f in files {
        zip.start_file(f.path.as_str(), options)?;
        zip.write_all(f.contents.as_bytes())?;
    }
    Ok(zip.finish()?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: with no target asked for and nothing generated, the error read "this project
    /// has no files for `any target`".
    #[test]
    fn a_project_nothing_is_generated_for_says_so() {
        let err = target_files(Vec::new(), None).unwrap_err();
        assert!(matches!(err, Error::NothingGenerated), "{err:?}");
        assert_eq!(err.to_string(), "no target generates any file for this project: the plan's `skipped` says why");
        let file = File {
            path: ".isoloom/docker/compose.yml".into(),
            contents: String::new(),
        };
        assert_eq!(target_files(vec![file.clone()], None).unwrap(), std::slice::from_ref(&file));
        assert!(matches!(target_files(vec![file], Some("vagrant")), Err(Error::NoFiles(t)) if t == "vagrant"));
    }
}
