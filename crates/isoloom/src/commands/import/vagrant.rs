//! `isoloom import vagrant`: a draft from a Vagrantfile, run against a stand-in `Vagrant` module.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use isoloom_core as core;

use super::draft::{Dest, source_of, write_draft};
use crate::Res;
use crate::tools::process;

pub fn run(file: Option<PathBuf>, dest: &Dest) -> Res<ExitCode> {
    let file = file.unwrap_or_else(|| PathBuf::from("Vagrantfile"));
    if !file.is_file() {
        return Err(format!("no {} here; pass the Vagrantfile", file.display()).into());
    }
    let recorded = record_vagrantfile(&file)?;
    let (folder, fallback, source) = source_of(&file)?;
    let draft = core::import::vagrant::draft(&recorded, &fallback, &source)?;
    write_draft(&draft, folder, dest)
}

/// Runs a Vagrantfile against a stand-in `Vagrant` module (in Vagrant's own Ruby when it's
/// installed) and returns every setting it made, as JSON. Nothing is created or started.
fn record_vagrantfile(file: &Path) -> Res<serde_json::Value> {
    const RECORDER: &str = include_str!("vagrant_record.rb");
    let dir = process::private_dir("isoloom-vagrant")?;
    let script = dir.join("record.rb");
    let recorded = std::fs::write(&script, RECORDER).map_err(Into::into).and_then(|()| run_recorder(&script, file));
    let _ = std::fs::remove_dir_all(&dir);
    recorded
}

/// The recorder run by the first Ruby found: Vagrant's own, else `ruby` on the PATH.
fn run_recorder(script: &Path, file: &Path) -> Res<serde_json::Value> {
    let rubies = [
        "/opt/vagrant/embedded/bin/ruby",
        "C:\\HashiCorp\\Vagrant\\embedded\\mingw64\\bin\\ruby.exe",
        "ruby",
    ];
    let mut last = String::from("no Ruby found (Vagrant's own, or `ruby` on the PATH)");
    for ruby in rubies {
        match crate::tools::process::command(ruby).arg(script).arg(file).output() {
            Ok(out) if out.status.success() => return Ok(serde_json::from_slice(&out.stdout)?),
            Ok(out) => {
                last = format!(
                    "the Vagrantfile failed to run: {}",
                    String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("")
                );
            }
            Err(_) => {}
        }
    }
    Err(last.into())
}
