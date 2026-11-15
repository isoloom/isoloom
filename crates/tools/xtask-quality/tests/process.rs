//! The process helpers the repositories' `xtask`s call: a command said on one line, its output
//! when it succeeds, its failure with what it said, and the program to start.

#![cfg(unix)]
#![expect(clippy::disallowed_methods, reason = "the tests start the commands the helpers run")]

use std::process::Command;

use xtask_quality::process::{describe, output, program, quiet};
use xtask_quality::repo::{remove, slashed};

fn sh(script: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(script);
    cmd
}

#[test]
fn a_command_is_said_on_one_line() {
    assert_eq!(describe(&sh("echo hi")), "sh -c echo hi");
}

#[test]
fn a_command_that_succeeds_gives_its_output_and_one_that_fails_says_so() {
    assert_eq!(output(&mut sh("echo hi")).ok().as_deref(), Some("hi\n"));
    let failed = output(&mut sh("exit 3")).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(failed.starts_with("`sh -c exit 3` failed"), "{failed}");
}

#[test]
fn a_quiet_command_that_fails_carries_the_end_of_what_it_said() {
    assert!(quiet(&mut sh("echo fine")).is_ok());
    let failed = quiet(&mut sh("for i in 1 2 3 4 5 6 7; do echo line$i >&2; done; exit 1"))
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(failed.ends_with("line3 / line4 / line5 / line6 / line7"), "{failed}");
}

#[test]
fn a_program_is_its_own_name_off_windows() {
    assert_eq!(program("npx", std::ffi::OsStr::new("/usr/bin")), "npx");
}

#[test]
fn a_path_is_written_with_forward_slashes() {
    assert_eq!(slashed(std::path::Path::new("a/b/c.rs")), "a/b/c.rs");
}

#[test]
fn removing_takes_a_file_or_a_folder_and_what_isnt_there_is_no_error() -> std::io::Result<()> {
    let root = tempfile::tempdir()?;
    let folder = root.path().join("f");
    std::fs::create_dir_all(folder.join("inner"))?;
    std::fs::write(folder.join("inner/x"), "")?;
    let file = root.path().join("file");
    std::fs::write(&file, "")?;
    remove(&folder)?;
    remove(&file)?;
    remove(&root.path().join("absent"))?;
    assert!(!folder.exists() && !file.exists());
    Ok(())
}
