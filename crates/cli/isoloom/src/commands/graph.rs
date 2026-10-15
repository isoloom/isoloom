//! `isoloom graph`: the environment drawn, as D2 or Graphviz source, or rendered to SVG/PNG
//! when the format's tool is installed.

use std::path::{Path, PathBuf};

use super::{CliError, Console, Project, Result, Verdict, spec};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: Project,
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

/// A diagram's format: its source language and the tool that renders it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    /// D2 (<https://d2lang.com>).
    D2,
    /// Graphviz DOT.
    Dot,
}

impl Format {
    /// The format named `name` (`d2`, `dot`).
    fn named(name: &str) -> Result<Format> {
        match name {
            "d2" => Ok(Format::D2),
            "dot" => Ok(Format::Dot),
            other => Err(CliError::UnknownFormat(other.to_owned())),
        }
    }

    /// The format's name, its source files' extension and its tool's name.
    fn name(self) -> &'static str {
        match self {
            Format::D2 => "d2",
            Format::Dot => "dot",
        }
    }

    /// Where its tool is installed from.
    fn install(self) -> &'static str {
        match self {
            Format::D2 => "https://d2lang.com/tour/install",
            Format::Dot => "https://graphviz.org/download/",
        }
    }

    /// The tool's arguments rendering `src` into `out`, a `.svg` or `.png` (`ext`).
    fn render_args(self, src: &Path, out: &Path, ext: &str) -> Vec<String> {
        match self {
            Format::D2 => vec![src.display().to_string(), out.display().to_string()],
            Format::Dot => vec![format!("-T{ext}"), "-o".into(), out.display().to_string(), src.display().to_string()],
        }
    }
}

/// Draws the environment: on stdout, into a source file, or rendered to an image.
pub(crate) fn run(a: &Args, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let format = Format::named(&a.format)?;
    let snapshot = spec::snapshot(profile, &a.project.dir, a.instance, &a.sets)?;
    let text = match format {
        Format::D2 => isoloom_targets::graph::d2(&snapshot),
        Format::Dot => isoloom_targets::graph::dot(&snapshot),
    };
    let Some(out) = &a.out else {
        console.print(text)?;
        return Ok(Verdict::Pass);
    };
    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("");
    if matches!(ext, "svg" | "png") {
        render(format, out, ext, &text)?;
        console.line(format_args!("✓ {} (source: {})", out.display(), out.with_extension(format.name()).display()))?;
    } else {
        std::fs::write(out, &text).map_err(CliError::writing(out))?;
        console.line(format_args!("✓ {}", out.display()))?;
    }
    Ok(Verdict::Pass)
}

/// Rendered by the format's own tool, from a sibling source file.
fn render(format: Format, out: &Path, ext: &str, text: &str) -> Result<()> {
    let src = out.with_extension(format.name());
    std::fs::write(&src, text).map_err(CliError::writing(&src))?;
    let program = format.name();
    match std::process::Command::new(program).args(format.render_args(&src, out, ext)).status() {
        Ok(st) if st.success() => Ok(()),
        Ok(st) => Err(CliError::ToolFailed {
            program: program.to_owned(),
            what: format!("rendering {} ({st}); the source is at {}", out.display(), src.display()),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(CliError::ToolMissing {
            program: program.to_owned(),
            what: format!("the diagram isn't rendered; the source is at {}", src.display()),
            install: format.install(),
        }),
        Err(e) => Err(CliError::ToolStart {
            program: program.to_owned(),
            source: e,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_format_is_d2_or_dot_and_another_is_named() {
        assert_eq!(Format::named("dot").ok(), Some(Format::Dot));
        assert!(matches!(Format::named("svg"), Err(CliError::UnknownFormat(f)) if f == "svg"));
    }

    #[test]
    fn each_tool_renders_the_source_into_the_image() {
        let (src, out) = (Path::new("lab.dot"), Path::new("lab.png"));
        assert_eq!(Format::Dot.render_args(src, out, "png"), ["-Tpng", "-o", "lab.png", "lab.dot"]);
        assert_eq!(Format::D2.render_args(Path::new("lab.d2"), Path::new("lab.svg"), "svg"), ["lab.d2", "lab.svg"]);
    }
}
