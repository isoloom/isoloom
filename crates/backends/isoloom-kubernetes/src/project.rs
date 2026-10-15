//! The project's own files a target carries with it: the Helm chart's copies of the scripts its
//! pods run (`chart/files/project/`, each at its path in the project), so the chart and the
//! kustomization (which reads the same copies) work away from the project. Generators only
//! see the spec, so whoever writes the files adds these: `isoloom generate`, `run` and `check`
//! do. The one place this crate reads the file system: the imperative shell around the pure
//! rendering.

use std::path::Path;

use isoloom_render::GeneratedFile;

use crate::error::{Error, Result};
use crate::kustomize;
use crate::layout::{DIR, PROJECT_COPIES};

/// The chart's copies of the project's scripts, read from the project folder `dir`, for the
/// kustomizations among `files`. A script the project doesn't have yet is left out: it is
/// `validate`'s to report (and the chart's `required` names it at install).
///
/// # Errors
///
/// [`Error::Kustomization`] when a generated kustomization isn't one (it was edited by hand),
/// [`Error::Read`] when a script exists but can't be read.
pub fn project_copies(files: &[GeneratedFile], dir: &Path) -> Result<Vec<GeneratedFile>> {
    let mut out = Vec::new();
    for f in files.iter().filter(|f| f.path.ends_with(&format!("/{DIR}/kustomization.yaml"))) {
        let base = f.path.trim_end_matches("kustomization.yaml");
        let scripts = kustomize::project_scripts(&f.contents).map_err(|source| Error::Kustomization { path: f.path.clone(), source })?;
        for path in scripts {
            let contents = match std::fs::read_to_string(dir.join(&path)) {
                Ok(c) => c,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(source) => return Err(Error::Read { path, source }),
            };
            out.push(GeneratedFile {
                path: format!("{base}{PROJECT_COPIES}/{path}"),
                contents,
            });
        }
    }
    Ok(out)
}
