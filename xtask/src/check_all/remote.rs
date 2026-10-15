//! The hosts reached over SSH: the Linux host (a bash leg) and the Windows host (a PowerShell
//! one). Each gets a fresh copy of the repositories, then runs `cargo xtask check-all --leg`.
//!
//! The copies and their build folders sit in a folder of their own per set of checkouts
//! ([`checkout::remote_folder`]): `<root>/<key>/src/<repo>` and `<root>/<key>/target/<repo>`,
//! with the checkouts listed in `<root>/<key>/checkouts`. The tar keeps the files' times, and
//! Cargo trusts times, so a build folder shared by two checkouts could keep test binaries built
//! from the other one's sources.
//!
//! Cargo compares a source's time with its last build's, written by the host's own clock. A host
//! whose clock is off ([`MAX_SKEW_S`]) would take a source changed here for older than its last
//! build, and test that build: its copies are dated when they land instead (`tar -m`), so every
//! run rebuilds them, and the log says the clock is off.

use std::fs::File;
use std::process::Command;

use super::checkout::{self, Folder};
use super::collect::{Collector, Sink};
use super::hosts::repos_arg;
use super::mirrors;
use super::pack::{self, Copy};
use super::settings::SshHost;
use super::{Repo, Run, Status};
use crate::{Res, process, repo, text};

/// How far a host's clock may be from this machine's, in seconds, before the files copied there
/// are dated when they land.
const MAX_SKEW_S: u64 = 120;

/// Whether the copies to a host whose clock reads `remote` (Unix seconds) when this machine's
/// reads `local` are dated when they land, and the note saying why: its clock is off.
fn dated_on_landing(local: u64, remote: u64) -> Option<String> {
    let (ahead, skew) = if remote >= local {
        ("ahead of", remote - local)
    } else {
        ("behind", local - remote)
    };
    (skew > MAX_SKEW_S).then(|| {
        format!(
            "note: this host's clock is {} min {ahead} this machine's: its copies are dated when they land, so everything rebuilds (set its clock right)",
            skew / 60
        )
    })
}

/// This machine's clock, in Unix seconds.
fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The Unix seconds a host printed (its first number), if any.
fn seconds_in(text: &str) -> Option<u64> {
    text.split_whitespace().find_map(|w| w.parse().ok())
}

/// The Windows host's folder when its settings name none.
const WIN_DIR: &str = "C:/isoloom-check-all";

fn win_dir(win: &SshHost) -> &str {
    win.dir.as_deref().unwrap_or(WIN_DIR)
}

/// `ssh` to `host`, never asking for anything.
pub(super) fn ssh(host: &SshHost) -> Command {
    let mut c = Command::new("ssh");
    c.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-o", "ServerAliveInterval=30"]);
    c.args(host.port_args("-p")).arg(host.target());
    c
}

/// A PowerShell script run on the Windows host, encoded (UTF-16LE in base64), so that neither
/// cmd nor PowerShell quoting applies.
pub(super) fn win_ps(win: &SshHost, script: &str) -> Command {
    let full = format!("$ProgressPreference = 'SilentlyContinue'; {script}");
    let utf16: Vec<u8> = full.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut c = ssh(win);
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

/// The checkouts a run copies from, Isoloom's first: its folder's key and marker.
fn checkouts(run: &Run) -> Vec<String> {
    [Repo::Isoloom, Repo::Isothrum, Repo::Launcher]
        .into_iter()
        .map(|r| repo::slashed(run.dirs.of(r)))
        .collect()
}

/// The marker's text, in base64, so no shell quoting applies to the paths.
fn marker_base64(run: &Run) -> String {
    text::base64(checkout::marker(&checkouts(run)).as_bytes())
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
    let host = &run.ssh.linux;
    let (home, clock) = match process::output(ssh(host).arg("cd && pwd && date +%s")) {
        Ok(said) => {
            let mut lines = said.lines();
            (lines.next().unwrap_or_default().trim().to_owned(), lines.next().and_then(seconds_in))
        }
        Err(_) => return out.host_failed(Status::Unreachable, "not reachable (set ISOLOOM_LINUX_USER and authorize the key)"),
    };
    let touch = clock.and_then(|remote| dated_on_landing(now(), remote));
    if let Some(note) = &touch {
        out.line(note);
    }
    if mirrors::needed(&run.repos) {
        match mirrors::sync_linux(run, &home) {
            Ok(note) => out.line(&note),
            Err(e) => {
                out.line(&e.to_string());
                return out.host_failed(Status::Fail, "syncing the Isoloom and Isothrum mirrors failed (see linux.log)");
            }
        }
    }
    let root = host.dir.clone().unwrap_or_else(|| format!("{home}/isoloom-check-all"));
    let folder = checkout::remote_folder(&root, &checkouts(run));
    out.line(&format!("copies and build folders: {folder}"));
    let mark = format!(
        "mkdir -p '{folder}' && echo '{}' | base64 -d > '{folder}/{}'",
        marker_base64(run),
        checkout::MARKER
    );
    if let Err(e) = process::quiet(ssh(host).arg(mark)) {
        out.line(&e.to_string());
        return out.host_failed(Status::Fail, &format!("can't write {folder}/{} (see linux.log)", checkout::MARKER));
    }
    for repo in shipped(run) {
        if let Err(e) = copy_linux(run, out, repo, &folder, touch.is_some()) {
            out.line(&e.to_string());
            return out.host_failed(Status::Fail, &format!("copying {} failed (see linux.log)", repo.id()));
        }
    }
    let src = format!("{folder}/src");
    let leg = format!(
        ". \"$HOME/.cargo/env\" 2>/dev/null; cd '{src}/isoloom' && CARGO_NET_GIT_FETCH_WITH_CLI=true CARGO_BUILD_JOBS={} CARGO_TARGET_DIR='{folder}/target/isoloom' \
         cargo xtask check-all --leg linux --profile platform --repos {} --isoloom '{src}/isoloom' --isothrum '{src}/isothrum' \
         --launcher '{src}/cyber-ctf-launcher' --target-root '{folder}/target'",
        host.jobs,
        repos_arg(run)
    );
    if let Err(e) = process::stream(ssh(host).arg(leg), &mut |l| out.feed(l)) {
        out.host_failed(Status::Fail, &format!("ssh couldn't start: {e}"));
    }
}

fn copy_linux(run: &Run, out: &mut Collector, repo: Repo, folder: &str, touch: bool) -> Res<()> {
    let tar = packed(run, out, repo, &format!("{folder}/src"))?;
    let dest = format!("{folder}/src/{}", repo.folder());
    let unpack = format!(
        "rm -rf '{dest}' && mkdir -p '{dest}' && tar {} --warning=no-unknown-keyword -C '{dest}'",
        extract_flags(touch, "-")
    );
    process::quiet(ssh(&run.ssh.linux).arg(unpack).stdin(File::open(&tar)?))?;
    std::fs::remove_file(tar)?;
    Ok(())
}

/// The Windows host's part.
pub(super) fn windows(run: &Run, out: &mut Collector) {
    let win = &run.ssh.windows;
    let dir = &checkout::remote_folder(win_dir(win), &checkouts(run));
    out.line(&format!("copies and build folders: {dir}"));
    let mark = format!(
        "New-Item -ItemType Directory -Force '{dir}/incoming' | Out-Null; \
         [IO.File]::WriteAllBytes('{dir}/{}', [Convert]::FromBase64String('{}')); [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()",
        checkout::MARKER,
        marker_base64(run)
    );
    let Ok(said) = process::output(&mut win_ps(win, &mark)) else {
        return out.host_failed(Status::Unreachable, "not reachable (set ISOLOOM_WIN_USER and authorize the key)");
    };
    let touch = seconds_in(&said).and_then(|remote| dated_on_landing(now(), remote));
    if let Some(note) = &touch {
        out.line(note);
    }
    if mirrors::needed(&run.repos) {
        match mirrors::sync_windows(run) {
            Ok(note) => out.line(&note),
            Err(e) => {
                out.line(&e.to_string());
                return out.host_failed(Status::Fail, "syncing the Isoloom and Isothrum mirrors failed (see windows.log)");
            }
        }
    }
    for repo in shipped(run) {
        if let Err(e) = copy_windows(run, out, repo, dir, touch.is_some()) {
            out.line(&e.to_string());
            return out.host_failed(Status::Fail, &format!("copying {} failed (see windows.log)", repo.id()));
        }
    }
    let src = format!("{dir}/src");
    let leg = format!(
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; $OutputEncoding = [Text.Encoding]::UTF8; \
         $env:CARGO_NET_GIT_FETCH_WITH_CLI = 'true'; $env:CARGO_BUILD_JOBS = '{}'; $env:CARGO_TARGET_DIR = '{dir}/target/isoloom'; \
         Set-Location '{src}/isoloom'; \
         & cargo xtask check-all --leg windows --profile platform --repos {} --isoloom '{src}/isoloom' --isothrum '{src}/isothrum' \
         --launcher '{src}/cyber-ctf-launcher' --target-root '{dir}/target'",
        win.jobs,
        repos_arg(run)
    );
    if let Err(e) = process::stream(&mut win_ps(win, &leg), &mut |l| out.feed(l)) {
        out.host_failed(Status::Fail, &format!("ssh couldn't start: {e}"));
    }
}

fn copy_windows(run: &Run, out: &mut Collector, repo: Repo, dir: &str, touch: bool) -> Res<()> {
    let win = &run.ssh.windows;
    let tar = packed(run, out, repo, &format!("{dir}/src"))?;
    let (name, dest) = (repo.folder(), format!("{dir}/src/{}", repo.folder()));
    process::quiet(
        Command::new("scp")
            .args(["-q", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10"])
            .args(win.port_args("-P"))
            .arg(&tar)
            .arg(format!("{}:{dir}/incoming/{name}.tar", win.target())),
    )?;
    let unpack = format!(
        "if (Test-Path '{dest}') {{ Remove-Item -Recurse -Force '{dest}' }}; New-Item -ItemType Directory -Force '{dest}' | Out-Null; \
         tar {} -C '{dest}'; if ($LASTEXITCODE) {{ exit 1 }}; Remove-Item '{dir}/incoming/{name}.tar'",
        extract_flags(touch, &format!("'{dir}/incoming/{name}.tar'"))
    );
    process::quiet(&mut win_ps(win, &unpack))?;
    std::fs::remove_file(tar)?;
    Ok(())
}

/// `tar`'s flags to extract `archive` (`-` for standard input): with `touch`, every file dated
/// when it lands (`-m`, which GNU tar and Windows' bsdtar both take) instead of keeping its time.
fn extract_flags(touch: bool, archive: &str) -> String {
    let dated = if touch { " -m" } else { "" };
    format!("-xf {archive}{dated}")
}

/// Removes the Linux host's folder.
pub(super) fn clean_linux(linux: &SshHost) -> Res<String> {
    let dir = linux.dir.as_deref().unwrap_or("$HOME/isoloom-check-all");
    process::quiet(ssh(linux).arg(format!("rm -rf {dir}")))?;
    Ok(format!("removed {dir}"))
}

/// Removes the Windows host's folder.
pub(super) fn clean_windows(win: &SshHost) -> Res<String> {
    let dir = win_dir(win);
    process::quiet(&mut win_ps(win, &format!("if (Test-Path '{dir}') {{ Remove-Item -Recurse -Force '{dir}' }}")))?;
    Ok(format!("removed {dir}"))
}

/// The folders of `root` to prune (see [`checkout::stale_folders`]), from its `listing`, then
/// `remove`d; what was removed.
fn prune(root: &str, listing: &str, remove: impl FnOnce(&[&str]) -> Res<()>) -> Res<String> {
    let folders: Vec<Folder> = checkout::parse_folders(listing);
    let stale = checkout::stale_folders(&folders, &|path| std::path::Path::new(path).is_dir());
    if stale.is_empty() {
        return Ok(format!("nothing to prune in {root}"));
    }
    remove(&stale)?;
    Ok(format!("pruned {} from {root}", stale.join(", ")))
}

/// Removes the Linux host's folders of checkouts no longer on this machine.
pub(super) fn prune_linux(linux: &SshHost) -> Res<String> {
    let root = linux.dir.as_deref().unwrap_or("$HOME/isoloom-check-all");
    let list = format!(
        "cd \"{root}\" 2>/dev/null || exit 0; for d in */; do [ -d \"$d\" ] || continue; d=${{d%/}}; echo \"{}$d\"; \
         [ -f \"$d/{marker}\" ] && cat \"$d/{marker}\"; done; true",
        checkout::FOLDER_LINE,
        marker = checkout::MARKER
    );
    let listing = process::output(ssh(linux).arg(list))?;
    prune(root, &listing, |stale| {
        let names: Vec<String> = stale.iter().map(|n| format!("'{n}'")).collect();
        process::quiet(ssh(linux).arg(format!("cd \"{root}\" && rm -rf -- {}", names.join(" "))))
    })
}

/// Removes the Windows host's folders of checkouts no longer on this machine.
pub(super) fn prune_windows(win: &SshHost) -> Res<String> {
    let root = win_dir(win);
    let list = format!(
        "if (Test-Path '{root}') {{ Get-ChildItem -Directory '{root}' | ForEach-Object {{ '{}' + $_.Name; \
         $m = Join-Path $_.FullName '{}'; if (Test-Path $m) {{ Get-Content $m }} }} }}",
        checkout::FOLDER_LINE,
        checkout::MARKER
    );
    let listing = process::output(&mut win_ps(win, &list))?;
    prune(root, &listing, |stale| {
        let names: Vec<String> = stale.iter().map(|n| format!("'{n}'")).collect();
        let remove = format!(
            "foreach ($n in @({})) {{ Remove-Item -Recurse -Force (Join-Path '{root}' $n) }}",
            names.join(", ")
        );
        process::quiet(&mut win_ps(win, &remove))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_whose_clock_is_off_gets_its_copies_dated_on_landing() {
        // karen, 2026-10-15: its clock seven hours ahead, so Cargo took the copied sources for
        // older than its last build, and tested the build of an earlier Isoloom.
        let note = dated_on_landing(1_000_000, 1_000_000 + 7 * 3600);
        assert!(note.as_deref().is_some_and(|n| n.contains("420 min ahead of")), "{note:?}");
        assert!(dated_on_landing(1_000_000, 1_000_000 - 600).is_some_and(|n| n.contains("10 min behind")));
        assert_eq!(dated_on_landing(1_000_000, 1_000_000 + MAX_SKEW_S), None, "a little drift keeps the times");
        assert_eq!(dated_on_landing(1_000_000, 1_000_000 - 30), None);
    }

    #[test]
    fn copies_keep_their_times_unless_dated_on_landing() {
        assert_eq!(extract_flags(false, "-"), "-xf -");
        assert_eq!(extract_flags(true, "'C:/x/incoming/isoloom.tar'"), "-xf 'C:/x/incoming/isoloom.tar' -m");
    }

    #[test]
    fn a_hosts_clock_is_the_first_number_it_prints() {
        assert_eq!(seconds_in("1792065600\r\n"), Some(1_792_065_600));
        assert_eq!(seconds_in("Directory: C:/x\n1792065600"), Some(1_792_065_600));
        assert_eq!(seconds_in(""), None);
    }
}
