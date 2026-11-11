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

/// The recording made by the first of [`RUBIES`] installed.
fn run_recorder(script: &Path, file: &Path) -> Result<Recording> {
    record_with(&RUBIES, script, file)
}

/// The recording made by the first of `rubies` that starts: one that isn't installed is passed
/// over, one that fails to start or to run the Vagrantfile is the error (another Ruby would only
/// hide it).
fn record_with(rubies: &[&str], script: &Path, file: &Path) -> Result<Recording> {
    for ruby in rubies {
        match std::process::Command::new(ruby).arg(script).arg(file).output() {
            Ok(out) if out.status.success() => return Ok(String::from_utf8_lossy(&out.stdout).parse::<Recording>()?),
            Ok(out) => {
                return Err(CliError::ToolFailed {
                    program: (*ruby).to_owned(),
                    what: format!(
                        "the Vagrantfile failed to run: {}",
                        String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("")
                    ),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(CliError::ToolStart {
                    program: (*ruby).to_owned(),
                    source,
                });
            }
        }
    }
    Err(CliError::ToolMissing {
        program: "ruby".to_owned(),
        what: "the Vagrantfile can't be run (Vagrant's own Ruby, or `ruby` on the PATH)".to_owned(),
        install: "https://developer.hashicorp.com/vagrant/install",
    })
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use super::{CliError, RECORDER, Recording, record_with};

    type Outcome = std::result::Result<(), Box<dyn std::error::Error>>;

    /// A fresh folder for one test, removed when it goes out of scope, with the recorder and
    /// `vagrantfile` in it.
    fn folder(name: &str, vagrantfile: &str) -> std::io::Result<(tempfile::TempDir, PathBuf, PathBuf)> {
        let dir = tempfile::Builder::new().prefix(&format!("isoloom-record-{name}-")).tempdir()?;
        let (script, file) = (dir.path().join("record.rb"), dir.path().join("Vagrantfile"));
        std::fs::write(&script, RECORDER)?;
        std::fs::write(&file, vagrantfile)?;
        Ok((dir, script, file))
    }

    /// A program at `path` that runs `body` with sh, executable or not.
    fn program(path: &Path, body: &str, executable: bool) -> std::io::Result<String> {
        std::fs::write(path, format!("#!/bin/sh\n{body}\n"))?;
        let mode = if executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
        Ok(path.to_string_lossy().into_owned())
    }

    /// The error of `recorded`, or a failure saying there was none.
    fn error(recorded: Result<Recording, CliError>) -> std::result::Result<CliError, Box<dyn std::error::Error>> {
        match recorded {
            Err(e) => Ok(e),
            Ok(_) => Err("recorded, where an error was expected".into()),
        }
    }

    #[test]
    fn the_first_ruby_that_starts_decides_and_a_missing_one_is_passed_over() -> Outcome {
        let (dir, script, file) = folder("first", "")?;
        let dir = dir.path();
        let failing = program(&dir.join("failing-ruby"), "echo 'undefined method' >&2; exit 1", true)?;
        let working = program(&dir.join("working-ruby"), "echo '{\"settings\":{},\"children\":{},\"calls\":[]}'", true)?;
        let missing = dir.join("no-such-ruby").to_string_lossy().into_owned();
        // A failing Vagrantfile is the error: the next Ruby isn't tried.
        let failed = error(record_with(&[&failing, &working], &script, &file))?;
        assert!(
            matches!(&failed, CliError::ToolFailed { what, .. } if what.ends_with("undefined method")),
            "{failed:?}"
        );
        // One that isn't installed is passed over.
        record_with(&[&missing, &working], &script, &file)?;
        let none = error(record_with(&[&missing], &script, &file))?;
        assert!(matches!(none, CliError::ToolMissing { .. }), "{none:?}");
        Ok(())
    }

    #[test]
    fn a_ruby_that_cant_start_is_the_error_not_skipped() -> Outcome {
        let (dir, script, file) = folder("start", "")?;
        let dir = dir.path();
        let unrunnable = program(&dir.join("unrunnable-ruby"), "exit 0", false)?;
        let e = error(record_with(&[&unrunnable, "ruby"], &script, &file))?;
        assert!(matches!(&e, CliError::ToolStart { program, .. } if *program == unrunnable), "{e:?}");
        Ok(())
    }

    #[test]
    fn what_a_vagrantfile_prints_never_reaches_the_recording() -> Outcome {
        let (_dir, script, file) = folder("puts", "puts \"hello\"\nVagrant.configure(\"2\") { |c| c.vm.box = \"debian/bookworm64\" }\n")?;
        let recorded = record_with(&["ruby"], &script, &file);
        match recorded {
            Err(CliError::ToolMissing { .. }) => eprintln!("skipped: needs ruby"),
            other => assert!(other.is_ok(), "{other:?}"),
        }
        Ok(())
    }
}
