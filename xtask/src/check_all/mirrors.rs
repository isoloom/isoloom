//! The Isoloom and Isothrum commits the consumers pin, on the SSH hosts.
//!
//! Isothrum and the launcher pin them by git (`https://github.com/isoloom/<repo>`), but local
//! `main` is never pushed, so GitHub hasn't got them. Before a run that copies a consumer, each SSH
//! host gets a bare mirror of this machine's Isoloom and Isothrum `main` in its home
//! (`~/isoloom-mirrors/<repo>.git`, fetched from a git bundle), and its global git configuration
//! rewrites the GitHub URL to that mirror (`url.<mirror>.insteadOf`), replacing any other rewrite of
//! the same URL. The legs run Cargo with `CARGO_NET_GIT_FETCH_WITH_CLI`, so it fetches through git,
//! which follows the rewrite.

use std::path::PathBuf;
use std::process::Command;

use super::remote::{ssh, win_ps};
use super::settings::SshHost;
use super::{Repo, Run};
use crate::{Res, process, text};

/// The repositories mirrored: the ones the consumers pin by git.
const MIRRORED: [Repo; 2] = [Repo::Isoloom, Repo::Isothrum];

/// Where the consumers' pins point.
const GITHUB: &str = "https://github.com/isoloom";

/// The mirrors' folder, in the host's home.
const FOLDER: &str = "isoloom-mirrors";

/// Whether a run of `repos` needs the mirrors: it copies a consumer of Isoloom.
#[must_use]
pub(super) fn needed(repos: &[Repo]) -> bool {
    repos.iter().any(|r| *r != Repo::Isoloom)
}

/// One repository's mirror on a host, and the rewrite that sends its GitHub URL there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Mirror {
    /// The repository.
    pub repo: Repo,
    /// The bare repository on the host, with `/`.
    pub dir: String,
    /// The bundle it is fetched from, beside it.
    pub bundle: String,
    /// The mirror's URL, which replaces the GitHub one.
    pub url: String,
    /// The GitHub URL the consumers pin.
    pub github: String,
}

impl Mirror {
    /// The git configuration key of the rewrite, as `git config --get-regexp` lists it.
    #[must_use]
    pub(super) fn key(&self) -> String {
        format!("url.{}.insteadof", self.url)
    }
}

/// The mirrors of a host whose home is `home` (`/home/me`, or `C:\Users\me` on Windows).
#[must_use]
pub(super) fn mirrors(home: &str) -> Vec<Mirror> {
    let home = home.trim().replace('\\', "/");
    let home = home.trim_end_matches('/');
    MIRRORED
        .into_iter()
        .map(|repo| {
            let base = format!("{home}/{FOLDER}/{}", repo.folder());
            let dir = format!("{base}.git");
            // A Unix path brings the third slash of `file:///`; a drive path (`C:/…`) doesn't.
            let url = if dir.starts_with('/') {
                format!("file://{dir}")
            } else {
                format!("file:///{dir}")
            };
            Mirror {
                repo,
                bundle: format!("{base}.bundle"),
                dir,
                url,
                github: format!("{GITHUB}/{}", repo.folder()),
            }
        })
        .collect()
}

/// The folder holding a host's mirrors.
fn parent(mirrors: &[Mirror]) -> String {
    mirrors
        .first()
        .and_then(|m| m.dir.rsplit_once('/'))
        .map(|(parent, _)| parent.to_owned())
        .unwrap_or_default()
}

/// The bash script that updates the Linux host's mirrors from their bundles, then points git at
/// them.
#[must_use]
pub(super) fn linux_script(mirrors: &[Mirror]) -> String {
    let steps: Vec<String> = mirrors
        .iter()
        .map(|m| {
            let (dir, bundle) = (text::sh_quote(&m.dir), text::sh_quote(&m.bundle));
            let (key, github) = (text::sh_quote(&m.key()), text::sh_quote(&m.github));
            format!(
                "[ -d {dir} ] || git init --quiet --bare {dir}\n\
                 git -C {dir} config uploadpack.allowAnySHA1InWant true\n\
                 git -C {dir} fetch --quiet --force {bundle} refs/heads/main:refs/heads/main\n\
                 git -C {dir} symbolic-ref HEAD refs/heads/main\n\
                 rm -f {bundle}\n\
                 rewrites=$(git config --global --get-regexp '^url\\..*\\.insteadof$' || true)\n\
                 printf '%s\\n' \"$rewrites\" | while read -r key value; do\n\
                 \x20 if [ \"$value\" = {github} ] && [ \"$key\" != {key} ]; then git config --global --unset-all \"$key\"; fi\n\
                 done\n\
                 git config --global --replace-all {key} {github}\n"
            )
        })
        .collect();
    format!("set -eu\n{}", steps.concat())
}

/// The PowerShell script that updates the Windows host's mirrors from their bundles, then points
/// git at them.
#[must_use]
pub(super) fn windows_script(mirrors: &[Mirror]) -> String {
    let steps: Vec<String> = mirrors
        .iter()
        .map(|m| {
            let (dir, bundle) = (text::ps_quote(&m.dir), text::ps_quote(&m.bundle));
            let (key, github) = (text::ps_quote(&m.key()), text::ps_quote(&m.github));
            format!(
                "if (-not (Test-Path -LiteralPath {dir})) {{ & git init --quiet --bare {dir} }}\n\
                 & git -C {dir} config uploadpack.allowAnySHA1InWant true\n\
                 & git -C {dir} fetch --quiet --force {bundle} 'refs/heads/main:refs/heads/main'\n\
                 if ($LASTEXITCODE) {{ throw ('fetching into ' + {dir} + ' failed') }}\n\
                 & git -C {dir} symbolic-ref HEAD refs/heads/main\n\
                 Remove-Item -LiteralPath {bundle}\n\
                 foreach ($line in @(& git config --global --get-regexp '^url\\..*\\.insteadof$')) {{\n\
                 \x20 $key, $value = $line -split ' ', 2\n\
                 \x20 if ($value -eq {github} -and $key -ne {key}) {{ & git config --global --unset-all $key }}\n\
                 }}\n\
                 & git config --global --replace-all {key} {github}\n\
                 if ($LASTEXITCODE) {{ throw ('pointing git at ' + {dir} + ' failed') }}\n"
            )
        })
        .collect();
    format!("$ErrorActionPreference = 'Stop'\n{}", steps.concat())
}

/// This machine's bundles of each mirrored repository's `main`, in the logs folder.
fn bundles(run: &Run, host: &str) -> Res<Vec<PathBuf>> {
    MIRRORED
        .into_iter()
        .map(|repo| {
            let file = run.logs.join(format!("{host}-{}.bundle", repo.folder()));
            process::quiet(
                Command::new("git")
                    .arg("-C")
                    .arg(run.dirs.of(repo))
                    .args(["bundle", "create", "--quiet"])
                    .arg(&file)
                    .arg("main"),
            )?;
            Ok(file)
        })
        .collect()
}

/// Copies `bundles` to their `mirrors` on `host`.
fn upload(host: &SshHost, bundles: &[PathBuf], mirrors: &[Mirror]) -> Res<()> {
    for (file, m) in bundles.iter().zip(mirrors) {
        process::quiet(
            Command::new("scp")
                .args(["-q", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10"])
                .args(host.port_args("-P"))
                .arg(file)
                .arg(format!("{}:{}", host.target(), m.bundle)),
        )?;
        std::fs::remove_file(file)?;
    }
    Ok(())
}

/// What a sync says in the host's log: each mirror's `main`.
fn synced(run: &Run) -> String {
    let heads: Vec<String> = MIRRORED
        .into_iter()
        .map(|repo| {
            let head = process::first_line(Command::new("git").arg("-C").arg(run.dirs.of(repo)).args(["rev-parse", "--short", "main"]));
            format!("{} main at {}", repo.id(), head.unwrap_or_else(|| "?".into()))
        })
        .collect();
    format!("mirrors: {}", heads.join(", "))
}

/// Brings the Linux host's mirrors to this machine's `main`, its home being `home`.
pub(super) fn sync_linux(run: &Run, home: &str) -> Res<String> {
    let host = &run.ssh.linux;
    let mirrors = mirrors(home);
    let files = bundles(run, "linux")?;
    process::quiet(ssh(host).arg(format!("mkdir -p {}", text::sh_quote(&parent(&mirrors)))))?;
    upload(host, &files, &mirrors)?;
    let script = text::base64(linux_script(&mirrors).as_bytes());
    process::quiet(ssh(host).arg(format!("echo {script} | base64 -d | bash")))?;
    Ok(synced(run))
}

/// Brings the Windows host's mirrors to this machine's `main`.
pub(super) fn sync_windows(run: &Run) -> Res<String> {
    let host = &run.ssh.windows;
    let home = process::output(&mut win_ps(host, "Write-Output $HOME"))?;
    let mirrors = mirrors(&home);
    let files = bundles(run, "windows")?;
    process::quiet(&mut win_ps(
        host,
        &format!("New-Item -ItemType Directory -Force {} | Out-Null", text::ps_quote(&parent(&mirrors))),
    ))?;
    upload(host, &files, &mirrors)?;
    process::quiet(&mut win_ps(host, &windows_script(&mirrors)))?;
    Ok(synced(run))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_run_that_copies_a_consumer_needs_the_mirrors() {
        assert!(!needed(&[Repo::Isoloom]));
        assert!(needed(&[Repo::Isoloom, Repo::Launcher]));
        assert!(needed(&[Repo::Isothrum]));
    }

    #[test]
    fn a_linux_home_rewrites_each_pinned_github_url_to_its_mirror() {
        let m = mirrors("/home/me\n");
        assert_eq!(
            m.iter().map(|m| (m.github.as_str(), m.url.as_str())).collect::<Vec<_>>(),
            [
                ("https://github.com/isoloom/isoloom", "file:///home/me/isoloom-mirrors/isoloom.git"),
                ("https://github.com/isoloom/isothrum", "file:///home/me/isoloom-mirrors/isothrum.git"),
            ]
        );
        assert_eq!(m[0].key(), "url.file:///home/me/isoloom-mirrors/isoloom.git.insteadof");
        assert_eq!(m[1].bundle, "/home/me/isoloom-mirrors/isothrum.bundle");
        assert_eq!(parent(&m), "/home/me/isoloom-mirrors");
    }

    #[test]
    fn a_windows_home_becomes_a_drive_url_with_forward_slashes() {
        let m = mirrors("C:\\Users\\vagrant\r\n");
        assert_eq!(m[0].dir, "C:/Users/vagrant/isoloom-mirrors/isoloom.git");
        assert_eq!(m[0].url, "file:///C:/Users/vagrant/isoloom-mirrors/isoloom.git");
        assert_eq!(m[0].key(), "url.file:///C:/Users/vagrant/isoloom-mirrors/isoloom.git.insteadof");
        assert_eq!(m[1].github, "https://github.com/isoloom/isothrum");
        assert_eq!(parent(&m), "C:/Users/vagrant/isoloom-mirrors");
    }

    #[test]
    fn the_scripts_fetch_each_bundle_and_leave_one_rewrite_per_url() {
        let linux = linux_script(&mirrors("/home/me"));
        assert!(
            linux.contains("git -C '/home/me/isoloom-mirrors/isoloom.git' fetch --quiet --force '/home/me/isoloom-mirrors/isoloom.bundle'"),
            "{linux}"
        );
        assert!(
            linux.contains("[ \"$key\" != 'url.file:///home/me/isoloom-mirrors/isothrum.git.insteadof' ]"),
            "{linux}"
        );
        assert!(
            linux.contains(
                "git config --global --replace-all 'url.file:///home/me/isoloom-mirrors/isothrum.git.insteadof' 'https://github.com/isoloom/isothrum'"
            ),
            "{linux}"
        );
        let windows = windows_script(&mirrors("C:\\Users\\vagrant"));
        assert!(
            windows.contains("& git -C 'C:/Users/vagrant/isoloom-mirrors/isoloom.git' fetch --quiet --force 'C:/Users/vagrant/isoloom-mirrors/isoloom.bundle'"),
            "{windows}"
        );
        assert!(
            windows
                .contains("$value -eq 'https://github.com/isoloom/isoloom' -and $key -ne 'url.file:///C:/Users/vagrant/isoloom-mirrors/isoloom.git.insteadof'"),
            "{windows}"
        );
    }
}
