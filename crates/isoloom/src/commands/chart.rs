//! `isoloom chart`: the Kubernetes files (the chart with them), then `helm package`.

use std::path::PathBuf;
use std::process::ExitCode;

use isoloom_core as core;

use crate::{Res, generated, output, spec};

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// The chart's version (and app version), e.g. the product's release: 4.2.1.
    #[arg(long, default_value = "0.1.0")]
    version: String,
    /// Where the .tgz goes (default: the project folder).
    #[arg(short, long, value_name = "DIR")]
    out: Option<PathBuf>,
    #[arg(long, value_name = "FILE")]
    images: Option<PathBuf>,
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let (dir, version) = (&a.dir, a.version.as_str());
    if version.is_empty() || version.starts_with('v') || !version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Err(format!("`--version {version}`: a semantic version, like 4.2.1").into());
    }
    let (spec, settings) = spec::load_settings(dir, a.images.as_deref(), &a.sets)?;
    if spec::report_invalid(&spec) {
        return Ok(ExitCode::FAILURE);
    }
    let files = core::generate(&spec, core::Target::Kubernetes).map_err(|e| format!("no chart: {e}"))?;
    generated::write_all(dir, &generated::finish(files, &settings.defaults, dir)?)?;
    let chart = dir.join(core::OUTPUT_DIR).join("kubernetes").join("chart");
    let dest = a.out.clone().unwrap_or_else(|| dir.clone());
    std::fs::create_dir_all(&dest)?;
    let status = crate::tools::process::command("helm")
        .arg("package")
        .arg(&chart)
        .args(["--version", version, "--app-version", version, "--destination"])
        .arg(&dest)
        .status()
        .map_err(|e| {
            format!(
                "can't run helm ({e}): install it (https://helm.sh/docs/intro/install/), or use {} as it is",
                chart.display()
            )
        })?;
    Ok(output::exit_code(status.success()))
}
