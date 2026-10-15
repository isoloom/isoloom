//! `cargo xtask host-setup`: what a test host needs for `cargo xtask check-all` and for the host
//! proof, installed once. What is there already is left alone, so running it again changes
//! nothing; `--check` only reports.
//!
//! - Linux ([`linux`]): without root, cargo-machete, then Terraform, kubectl, kind, Helm,
//!   PowerShell 7, and Ansible with ansible-lint (through pipx), in `~/.local/bin`, each download
//!   checked against the SHA-256 its publisher lists. The system packages that need root (the
//!   launcher's Tauri build, PowerShell's ICU) are printed as the apt command to run.
//! - Windows: `xtask/host-setup/windows.ps1`, as an administrator: git, the Visual Studio Build
//!   Tools, Rust, cargo-machete, Node 24 and pnpm, Vagrant, Terraform, PowerShell 7 and long
//!   paths, from winget. On a fresh host, with no Rust yet, run that script directly first.
//! - macOS needs nothing: it runs every gate as it is.

mod linux;

use std::process::{Command, ExitCode};

use crate::{Res, repo};

/// The arguments of `cargo xtask host-setup`.
#[derive(clap::Args)]
pub struct Args {
    /// Report what is there and what is missing; change nothing.
    #[arg(long)]
    check: bool,
}

/// Sets up (or checks) this host.
pub fn run(a: &Args) -> Res<ExitCode> {
    match std::env::consts::OS {
        "linux" => linux::run(a.check),
        "windows" => windows(a.check),
        other => {
            println!("nothing to set up on {other}: host-setup prepares the Linux and Windows test hosts");
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn windows(check: bool) -> Res<ExitCode> {
    let script = repo::root().join("xtask/host-setup/windows.ps1");
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(script);
    if check {
        cmd.arg("-CheckOnly");
    }
    let status = cmd.status()?;
    Ok(if status.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
