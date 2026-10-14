//! The Kubernetes backend: manifests, Kustomize and the Helm chart.

pub mod kubernetes;

use isoloom_compose::docker;
use isoloom_plan::checks;
use isoloom_render::{GenerateError, GeneratedFile, OUTPUT_DIR, constraints, container_checks_unsupported, header};
use isoloom_spec::{model, shell};
pub use kubernetes::{CHART, CHART_PROJECT_FILES, generate};

/// The project's own files a target carries with it, read from the project folder `dir`: the
/// Helm chart's copies of the scripts its pods run (`chart/files/project/`), so the chart works
/// away from the project. Generators only see the spec, so whoever writes `files` adds these:
/// `isoloom generate`, `run` and `check` do.
pub fn project_copies(files: &[GeneratedFile], dir: &std::path::Path) -> Result<Vec<GeneratedFile>, String> {
    let mut out = Vec::new();
    for f in files.iter().filter(|f| f.path.ends_with("/kubernetes/kustomization.yaml")) {
        let base = f.path.trim_end_matches("kustomization.yaml");
        // The kustomization lists them as `- <key>=../../<path in the project>`.
        for line in f.contents.lines() {
            let Some((key, path)) = line.trim().strip_prefix("- ").and_then(|l| l.split_once('=')) else {
                continue;
            };
            let Some(path) = path.strip_prefix("../../") else { continue };
            // A file the project doesn't have yet is `validate`'s to report (and the chart's
            // `required` names it at install): generating goes on without it.
            let contents = match std::fs::read_to_string(dir.join(path)) {
                Ok(c) => c,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(format!("{path} (copied into the Helm chart): {e}")),
            };
            out.push(GeneratedFile {
                path: format!("{base}{CHART}/{CHART_PROJECT_FILES}/{key}"),
                contents,
            });
        }
    }
    Ok(out)
}
