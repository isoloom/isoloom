//! `isoloom import vagrant`: a draft from a Vagrantfile, run against a stand-in `Vagrant` module.

use std::path::{Path, PathBuf};

use isoloom_import::vagrant::Recording;

use super::draft::{self, Dest, Source};
use crate::console::Console;
use crate::error::{CliError, Result};
use crate::exit::Verdict;

/// The recorder: Ruby that runs a Vagrantfile against a stand-in `Vagrant` module and prints
/// every setting it made, as JSON.
const RECORDER: &str = include_str!("vagrant_record.rb");

/// The Rubies tried, in order: Vagrant's own (macOS and Linux, then Windows), then the PATH's.
const RUBIES: [&str; 3] = [
    "/opt/vagrant/embedded/bin/ruby",
    "C:\\HashiCorp\\Vagrant\\embedded\\mingw64\\bin\\ruby.exe",
    "ruby",
];

/// Drafts the spec from `file`, else from `./Vagrantfile`.
pub(crate) fn run(file: Option<PathBuf>, dest: &Dest, console: &mut Console) -> Result<Verdict> {
    let file = file.unwrap_or_else(|| PathBuf::from("Vagrantfile"));
    if !file.is_file() {
        return Err(CliError::NothingToImport(format!("no {} here; pass the Vagrantfile", file.display())));
    }
    let recording = record(&file, console)?;
    let source = Source::file(&file)?;
    let drafted = isoloom_import::vagrant::draft(&recording, &source.fallback, &source.named)?;
    draft::write(&drafted, &source.folder, dest, console)
}

/// Runs a Vagrantfile against the recorder (in Vagrant's own Ruby when it's installed) and
/// returns every setting it made. Nothing is created or started.
fn record(file: &Path, console: &mut Console) -> Result<Recording> {
    let dir = private_dir("isoloom-vagrant")?;
    let script = dir.join("record.rb");
    let recorded = std::fs::write(&script, RECORDER)
        .map_err(CliError::writing(&script))
        .and_then(|()| run_recorder(&script, file));
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        console.note(format_args!("note: {} is left behind: {e}\n", dir.display()))?;
    }
    recorded
}

/// A private folder under the system's temporary folder, `<prefix>-<pid>-<nonce>`, created
/// exclusively (it fails if it exists), so a local attacker can't pre-place a symlink at a
/// guessable path and have the recorder swapped.
fn private_dir(prefix: &str) -> Result<PathBuf> {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nonce:x}", std::process::id()));
    std::fs::create_dir(&dir).map_err(CliError::writing(&dir))?;
    Ok(dir)
}

/// The recording made by the first Ruby found.
fn run_recorder(script: &Path, file: &Path) -> Result<Recording> {
    let mut failed = None;
    for ruby in RUBIES {
        match std::process::Command::new(ruby).arg(script).arg(file).output() {
            Ok(out) if out.status.success() => return Ok(String::from_utf8_lossy(&out.stdout).parse::<Recording>()?),
            Ok(out) => {
                failed = Some(CliError::ToolFailed {
                    program: ruby.to_owned(),
                    what: format!(
                        "the Vagrantfile failed to run: {}",
                        String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("")
                    ),
                });
            }
            Err(_) => {}
        }
    }
    Err(failed.unwrap_or(CliError::ToolMissing {
        program: "ruby".to_owned(),
        what: "the Vagrantfile can't be run (Vagrant's own Ruby, or `ruby` on the PATH)".to_owned(),
        install: "https://developer.hashicorp.com/vagrant/install",
    }))
}
