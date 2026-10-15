//! Where a draft goes, and what is said about it: its notes by kind, and whether it validates.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::Res;

/// Where to put the draft: `--out`, `--force`, `--stdout`.
pub struct Dest {
    pub out: Option<PathBuf>,
    pub force: bool,
    pub stdout: bool,
}

/// The folder a source file is in, a name for the environment (the folder's), and how the
/// draft's header names the file.
pub fn source_of(file: &Path) -> Res<(PathBuf, String, String)> {
    let folder = file.canonicalize()?.parent().map(PathBuf::from).unwrap_or_default();
    let fallback = folder.file_name().and_then(|n| n.to_str()).unwrap_or("env").to_string();
    let source = file.file_name().and_then(|n| n.to_str()).unwrap_or("the source").to_string();
    Ok((folder, fallback, source))
}

/// Writes a draft (or prints it), then its notes by kind and whether it validates.
pub fn write_draft(draft: &isoloom_import::Draft, folder: PathBuf, dest: &Dest) -> Res<ExitCode> {
    if dest.stdout {
        print!("{}", draft.yaml);
    } else {
        let dir = dest.out.clone().unwrap_or(folder);
        let target = dir.join("isoloom.yml");
        if (target.exists() || dir.join("isoloom.yaml").exists()) && !dest.force {
            return Err(format!("{} already has a spec; use --force to replace it, or --stdout", dir.display()).into());
        }
        std::fs::write(&target, &draft.yaml)?;
        println!("✓ wrote {}", target.display());
    }
    let report = report(draft);
    if dest.stdout {
        eprint!("{report}");
    } else {
        print!("{report}");
    }
    Ok(ExitCode::SUCCESS)
}

/// The draft's notes by kind, then whether it validates.
fn report(draft: &isoloom_import::Draft) -> String {
    let mut report = String::new();
    for kind in isoloom_import::NoteKind::ALL {
        let notes: Vec<_> = draft.notes.iter().filter(|n| n.kind == kind).collect();
        if notes.is_empty() {
            continue;
        }
        let _ = writeln!(report, "\n{} ({})", kind.title(), notes.len());
        for n in notes {
            let _ = writeln!(report, "  {}: {}", n.at, n.text);
        }
    }
    let problems = match isoloom_spec::syntax::parse(&draft.yaml) {
        Ok(spec) => isoloom_targets::Lab::new(spec).problems(),
        Err(problems) => problems,
    };
    if problems.is_empty() {
        report.push_str("\n✓ the draft is valid\n");
    } else {
        report.push_str("\nThe draft needs fixing before it validates:\n");
        for p in &problems {
            let _ = writeln!(report, "  {}", p.line());
        }
    }
    report
}
