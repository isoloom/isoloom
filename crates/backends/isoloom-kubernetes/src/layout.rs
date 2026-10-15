//! Where the target's files go: `.isoloom/kubernetes/` in the project, the chart in its `chart/`
//! folder, and the paths they read the project by.

use isoloom_render::OUTPUT_DIR;

/// The target's folder, under `.isoloom/`.
pub(crate) const DIR: &str = "kubernetes";
/// Where the chart is, under the target's folder.
pub(crate) const CHART: &str = "chart";
/// Where the chart's copies of the project's scripts go, under the chart.
pub(crate) const CHART_PROJECT_FILES: &str = "files/project";
/// From `.isoloom/kubernetes/` back to the project folder.
pub(crate) const ROOT: &str = "../..";

/// Where a file of the target goes, from the project folder.
pub(crate) fn output(path: &str) -> String {
    format!("{OUTPUT_DIR}/{DIR}/{path}")
}
