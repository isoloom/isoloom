//! The hosts reached over SSH: the Linux host (a bash leg) and the Windows host (a PowerShell
//! one). Each gets a fresh copy of the repositories, then runs `cargo xtask check-all --leg`.

use std::fs::File;
use std::process::Command;

use super::collect::{Collector, Sink};
use super::hosts::{jobs, repos_arg};
use super::pack::{self, Copy};
use super::{Repo, Run, Status};
use crate::{Res, process, text};

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_owned())
}

fn linux_target() -> String {
    format!(
        "{}@{}",
        env_or("ISOLOOM_LINUX_USER", "florianamette"),
        env_or("ISOLOOM_LINUX_HOST", "10.0.0.10")
    )
}

fn win_target() -> String {
    format!("{}@{}", env_or("ISOLOOM_WIN_USER", "florianamette"), env_or("ISOLOOM_WIN_HOST", "10.0.0.40"))
}

fn win_dir() -> String {
    env_or("ISOLOOM_WIN_DIR", "C:/isoloom-check-all")
}

fn ssh(target: &str) -> Command {
    let mut c = Command::new("ssh");
    c.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-o", "ServerAliveInterval=30", target]);
    c
}

/// A PowerShell script run on the Windows host, encoded (UTF-16LE in base64), so that neither
/// cmd nor PowerShell quoting applies.
fn win_ps(script: &str) -> Command {
    let full = format!("$ProgressPreference = 'SilentlyContinue'; {script}");
    let utf16: Vec<u8> = full.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut c = ssh(&win_target());
    c.arg(format!(
        "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {}",
        text::base64(&utf16)
    ));
    c
}

/// The repositories a remote host needs: Isoloom always (it runs the leg), then the others.
fn shipped(run: &Run) -> Vec<Repo> {
    let mut repos = vec![Repo::Isoloom];
    repos.extend(run.repos.iter().copied().filter(|r| *r != Repo::Isoloom));
    repos
}

/// Packs `repo` for a host whose copies sit under `root`, into the logs folder.
fn packed(run: &Run, out: &mut Collector, repo: Repo, root: &str) -> Res<std::path::PathBuf> {
    let tar = run.logs.join(format!("{}-{}.tar", out.host().id(), repo.id()));
    let copy = Copy {
        name: repo.folder(),
        dir: run.dirs.of(repo),
        root,
        isoloom: &run.dirs.isoloom,
    };
    for note in pack::pack(&copy, &tar)? {
        out.line(&note);
    }
    Ok(tar)
}

/// The Linux host's part.
pub(super) fn linux(run: &Run, out: &mut Collector) {
    let target = linux_target();
    let home = match process::output(ssh(&target).arg("cd && pwd")) {
        Ok(home) => home.trim().to_owned(),
        Err(_) => return out.host_failed(Status::Unreachable, "not reachable (set ISOLOOM_LINUX_USER and authorize the key)"),
    };
    let root = std::env::var("ISOLOOM_LINUX_DIR").unwrap_or_else(|_| format!("{home}/isoloom-check-all"));
    for repo in shipped(run) {
        if let Err(e) = copy_linux(run, out, repo, &root) {
            out.line(&e.to_string());
            return out.host_failed(Status::Fail, &format!("copying {} failed (see linux.log)", repo.id()));
        }
    }
    let src = format!("{root}/src");
    let leg = format!(
        ". \"$HOME/.cargo/env\" 2>/dev/null; cd '{src}/isoloom' && CARGO_BUILD_JOBS={} CARGO_TARGET_DIR='{root}/target/isoloom' \
         cargo xtask check-all --leg linux --profile platform --repos {} --isoloom '{src}/isoloom' --isothrum '{src}/isothrum' \
         --launcher '{src}/cyber-ctf-launcher' --target-root '{root}/target'",
        jobs(),
        repos_arg(run)
    );
    if let Err(e) = process::stream(ssh(&target).arg(leg), &mut |l| out.feed(l)) {
        out.host_failed(Status::Fail, &format!("ssh couldn't start: {e}"));
    }
}

fn copy_linux(run: &Run, out: &mut Collector, repo: Repo, root: &str) -> Res<()> {
    let tar = packed(run, out, repo, &format!("{root}/src"))?;
    let dest = format!("{root}/src/{}", repo.folder());
    let unpack = format!("rm -rf '{dest}' && mkdir -p '{dest}' && tar -xf - --warning=no-unknown-keyword -C '{dest}'");
    process::quiet(ssh(&linux_target()).arg(unpack).stdin(File::open(&tar)?))?;
    std::fs::remove_file(tar)?;
    Ok(())
}

/// The Windows host's part.
pub(super) fn windows(run: &Run, out: &mut Collector) {
    let dir = win_dir();
    if process::quiet(&mut win_ps(&format!("New-Item -ItemType Directory -Force '{dir}/incoming' | Out-Null"))).is_err() {
        return out.host_failed(Status::Unreachable, "not reachable (set ISOLOOM_WIN_USER and authorize the key)");
    }
    for repo in shipped(run) {
        if let Err(e) = copy_windows(run, out, repo, &dir) {
            out.line(&e.to_string());
            return out.host_failed(Status::Fail, &format!("copying {} failed (see windows.log)", repo.id()));
        }
    }
    let src = format!("{dir}/src");
    let leg = format!(
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; $OutputEncoding = [Text.Encoding]::UTF8; \
         $env:CARGO_BUILD_JOBS = '{}'; $env:CARGO_TARGET_DIR = '{dir}/target/isoloom'; Set-Location '{src}/isoloom'; \
         & cargo xtask check-all --leg windows --profile platform --repos {} --isoloom '{src}/isoloom' --isothrum '{src}/isothrum' \
         --launcher '{src}/cyber-ctf-launcher' --target-root '{dir}/target'",
        jobs(),
        repos_arg(run)
    );
    if let Err(e) = process::stream(&mut win_ps(&leg), &mut |l| out.feed(l)) {
        out.host_failed(Status::Fail, &format!("ssh couldn't start: {e}"));
    }
}

fn copy_windows(run: &Run, out: &mut Collector, repo: Repo, dir: &str) -> Res<()> {
    let tar = packed(run, out, repo, &format!("{dir}/src"))?;
    let (name, dest) = (repo.folder(), format!("{dir}/src/{}", repo.folder()));
    process::quiet(
        Command::new("scp")
            .args(["-q", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10"])
            .arg(&tar)
            .arg(format!("{}:{dir}/incoming/{name}.tar", win_target())),
    )?;
    let unpack = format!(
        "if (Test-Path '{dest}') {{ Remove-Item -Recurse -Force '{dest}' }}; New-Item -ItemType Directory -Force '{dest}' | Out-Null; \
         tar -xf '{dir}/incoming/{name}.tar' -C '{dest}'; if ($LASTEXITCODE) {{ exit 1 }}; Remove-Item '{dir}/incoming/{name}.tar'"
    );
    process::quiet(&mut win_ps(&unpack))?;
    std::fs::remove_file(tar)?;
    Ok(())
}

/// Removes the Linux host's folder.
pub(super) fn clean_linux() -> Res<String> {
    let dir = std::env::var("ISOLOOM_LINUX_DIR").unwrap_or_else(|_| "$HOME/isoloom-check-all".into());
    process::quiet(ssh(&linux_target()).arg(format!("rm -rf {dir}")))?;
    Ok(format!("removed {dir}"))
}

/// Removes the Windows host's folder.
pub(super) fn clean_windows() -> Res<String> {
    let dir = win_dir();
    process::quiet(&mut win_ps(&format!("if (Test-Path '{dir}') {{ Remove-Item -Recurse -Force '{dir}' }}")))?;
    Ok(format!("removed {dir}"))
}
