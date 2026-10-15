//! `isoloom graph`: the environment drawn, as D2 or Graphviz source, or rendered to SVG/PNG
//! when the format's tool is installed.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::{Res, spec};

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// `d2` (default) or `dot`.
    #[arg(long, default_value = "d2")]
    format: String,
    /// Write here instead of stdout; `.svg` and `.png` are rendered.
    #[arg(short = 'o', long)]
    out: Option<PathBuf>,
    /// Draw instance N (its blocks and name).
    #[arg(long, value_name = "N")]
    instance: Option<u8>,
    /// Overrides, as for `generate`.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

pub fn run(a: &Args, profile: &spec::Profile) -> Res<ExitCode> {
    let snapshot = spec::snapshot_of(profile, &a.dir, a.instance, &a.sets)?;
    let text = match a.format.as_str() {
        "d2" => isoloom_targets::report::graph_d2(&snapshot),
        "dot" => isoloom_targets::report::graph_dot(&snapshot),
        other => return Err(format!("unknown format `{other}`; `d2` or `dot`").into()),
    };
    let Some(out) = &a.out else {
        print!("{text}");
        return Ok(ExitCode::SUCCESS);
    };
    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("");
    if matches!(ext, "svg" | "png") {
        return render(&a.format, out, ext, &text);
    }
    std::fs::write(out, &text)?;
    println!("✓ {}", out.display());
    Ok(ExitCode::SUCCESS)
}

/// Rendered by the format's own tool, from a sibling source file.
fn render(format: &str, out: &Path, ext: &str, text: &str) -> Res<ExitCode> {
    let src = out.with_extension(format);
    std::fs::write(&src, text)?;
    let (program, args): (&str, Vec<String>) = match format {
        "d2" => ("d2", vec![src.display().to_string(), out.display().to_string()]),
        _ => (
            "dot",
            vec![format!("-T{ext}"), "-o".into(), out.display().to_string(), src.display().to_string()],
        ),
    };
    match std::process::Command::new(program).args(&args).status() {
        Ok(st) if st.success() => {
            println!("✓ {} (source: {})", out.display(), src.display());
            Ok(ExitCode::SUCCESS)
        }
        Ok(_) => Err(format!("{program} failed; the source is at {}", src.display()).into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "`{program}` isn't installed, so the diagram isn't rendered; the source is at {} ({})",
            src.display(),
            if format == "d2" {
                "https://d2lang.com/tour/install"
            } else {
                "https://graphviz.org/download/"
            }
        )
        .into()),
        Err(e) => Err(e.into()),
    }
}
