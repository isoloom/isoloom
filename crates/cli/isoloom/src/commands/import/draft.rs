//! Where a draft goes, and what is said about it: its notes by kind, and whether it passes
//! `isoloom validate`.

use std::path::{Path, PathBuf};

use isoloom_import::{Draft, NoteKind};

use crate::console::Console;
use crate::error::{CliError, Result};
use crate::exit::Verdict;

#[derive(Debug, clap::Args)]
pub(crate) struct Dest {
    /// Where to write isoloom.yml (default: next to the source).
    #[arg(long)]
    out: Option<PathBuf>,
    /// Replace an existing isoloom.yml.
    #[arg(long)]
    force: bool,
    /// Print the draft instead of writing it.
    #[arg(long)]
    stdout: bool,
}

/// The source file's folder, a name for the environment (the folder's), and how the draft's
/// header names the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Source {
    /// The folder the source is in: where the draft goes by default.
    pub(crate) folder: PathBuf,
    /// The environment's name when the source gives none.
    pub(crate) fallback: String,
    /// How the draft's header names the source.
    pub(crate) named: String,
}

impl Source {
    /// The source file `file`.
    pub(crate) fn file(file: &Path) -> Result<Source> {
        let folder = file
            .canonicalize()
            .map_err(CliError::reading(file))?
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();
        let named = file.file_name().and_then(|n| n.to_str()).unwrap_or("the source").to_owned();
        Ok(Source {
            fallback: name_of(&folder),
            folder,
            named,
        })
    }

    /// The source folder `dir`, its files named `<folder> (<what>)`.
    pub(crate) fn folder(dir: &Path, what: &str) -> Result<Source> {
        let folder = dir.canonicalize().map_err(CliError::reading(dir))?;
        let name = folder.file_name().and_then(|n| n.to_str()).unwrap_or(".").to_owned();
        Ok(Source {
            fallback: name_of(&folder),
            named: format!("{name} ({what})"),
            folder,
        })
    }
}

/// A folder's name, as an environment's name (`env` when it has none).
fn name_of(folder: &Path) -> String {
    folder.file_name().and_then(|n| n.to_str()).unwrap_or("env").to_owned()
}

/// Writes a draft (or prints it), then its notes by kind and whether it validates: on the
/// output, or beside the draft when the draft is the output.
pub(crate) fn write(draft: &Draft, folder: &Path, dest: &Dest, console: &mut Console) -> Result<Verdict> {
    let report = report(draft);
    if dest.stdout {
        console.print(&draft.yaml)?;
        console.note(report)?;
        return Ok(Verdict::Pass);
    }
    let dir = dest.out.as_deref().unwrap_or(folder);
    let target = dir.join("isoloom.yml");
    if (target.exists() || dir.join("isoloom.yaml").exists()) && !dest.force {
        return Err(CliError::SpecExists(dir.to_path_buf()));
    }
    std::fs::write(&target, &draft.yaml).map_err(CliError::writing(&target))?;
    console.line(format_args!("✓ wrote {}", target.display()))?;
    console.print(report)?;
    Ok(Verdict::Pass)
}

/// The draft's notes by kind, then whether it passes `isoloom validate`.
fn report(draft: &Draft) -> String {
    let mut lines = Vec::new();
    for kind in NoteKind::ALL {
        let notes: Vec<_> = draft.notes.iter().filter(|n| n.kind == kind).collect();
        if notes.is_empty() {
            continue;
        }
        lines.push(format!("\n{kind} ({})", notes.len()));
        lines.extend(notes.iter().map(|n| format!("  {}: {}", n.at, n.text)));
    }
    let problems = match isoloom_spec::syntax::parse(&draft.yaml) {
        Ok(spec) => isoloom_targets::Lab::new(spec).problems(),
        Err(problems) => problems,
    };
    if problems.is_empty() {
        lines.push("\n✓ the draft is valid".to_owned());
    } else {
        lines.push("\nThe draft needs fixing before it validates:".to_owned());
        lines.extend(problems.iter().map(|p| format!("  {}", p.line())));
    }
    format!("{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_lists_the_notes_by_kind_then_says_the_draft_is_valid() -> std::result::Result<(), isoloom_import::Error> {
        let draft = isoloom_import::compose::draft("services:\n  web:\n    image: nginx\n    command: run\n", "lab", "compose.yaml")?;
        let report = report(&draft);
        assert!(
            report.contains("\nBelongs in the image (1)\n  services.web.command: set it in the image"),
            "{report}"
        );
        assert!(report.ends_with("\n✓ the draft is valid\n"), "{report}");
        Ok(())
    }
}
