//! The project's own files a target carries with it: the Helm chart's copies of the scripts its
//! pods run (`chart/files/project/`, each at its path in the project), so the chart and the
//! kustomization (which reads the same copies) work away from the project. Generators only
//! see the spec, so whoever writes the files adds these: `isoloom generate`, `run` and `check`
//! do. The one place this crate reads the file system: the imperative shell around the pure
//! rendering.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::GeneratedFile;

use crate::checks;
use crate::error::{Error, Result};
use crate::layout::{PROJECT_COPIES, output};
use crate::machine::Machine;
use crate::render::project_scripts;

/// The chart's copies of the project's scripts `env`'s pods run, read from the project folder
/// `dir`, where the target writes them (`.isoloom/kubernetes/chart/files/project/`). A script the
/// project doesn't have yet is left out: it is `validate`'s to report (and the chart's `required`
/// names it at install).
///
/// # Errors
///
/// [`Error::Read`] when a script exists but can't be read.
pub fn project_copies(env: &Environment, dir: &Path) -> Result<Vec<GeneratedFile>> {
    let mut out = Vec::new();
    for path in project_scripts(&Machine::all(env), &checks::plan(env)) {
        let contents = match std::fs::read_to_string(dir.join(&path)) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(Error::Read { path, source }),
        };
        out.push(GeneratedFile {
            path: output(&format!("{PROJECT_COPIES}/{path}")),
            contents,
        });
    }
    Ok(out)
}
