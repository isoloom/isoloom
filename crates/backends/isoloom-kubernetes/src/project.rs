//! The project's own files a target carries with it: the Helm chart's copies of the scripts its
//! pods run (`chart/files/project/`, each at its path in the project), so the chart and the
//! kustomization (which reads the same copies) work away from the project. Generators only
//! see the spec, so whoever writes the files adds these: `isoloom generate` and `check` do
//! (through `isoloom_targets::project_copies`). The one place this crate reads the file system:
//! the imperative shell around the pure rendering.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::GeneratedFile;

use crate::checks;
use crate::error::{Error, Result};
use crate::layout::{PROJECT_COPIES, output};
use crate::machine::Machine;
use crate::render::project_scripts;

/// The chart's copies of the project's scripts `env`'s pods run, read from the project folder
/// `dir`, where the target writes them (`.isoloom/kubernetes/chart/files/project/`).
///
/// # Errors
///
/// [`Error::Outside`] when a script isn't there or leads out of the project folder (through a
/// link: what it leads to is never read), [`Error::Read`] when it can't be read: the chart and
/// the kustomization would name a file they don't hold.
pub fn project_copies(env: &Environment, dir: &Path) -> Result<Vec<GeneratedFile>> {
    let mut out = Vec::new();
    for path in project_scripts(&Machine::all(env), &checks::plan(env)) {
        // Read where the path really leads, links followed, and only inside the project folder.
        let real = isoloom_spec::project_path::inside(dir, &path).map_err(|why| Error::Outside { path: path.clone(), why })?;
        let contents = std::fs::read_to_string(&real).map_err(|source| Error::Read { path: path.clone(), source })?;
        out.push(GeneratedFile {
            path: output(&format!("{PROJECT_COPIES}/{path}")),
            contents,
        });
    }
    Ok(out)
}
