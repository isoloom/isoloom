//! `cargo xtask check-all`: the gates of every repository (Isoloom, Isothrum, the Cyber CTF
//! launcher) on one or more hosts, then one summary table. Exits 1 when a gate fails.
//!
//! Hosts (default: mac):
//! - `mac`: this machine, natively, every gate (the full profile).
//! - `linux`: a Linux host over SSH: `ISOLOOM_LINUX_HOST` (10.0.0.10), `ISOLOOM_LINUX_USER`
//!   (florianamette), `ISOLOOM_LINUX_PORT`, sources and build folders under `ISOLOOM_LINUX_DIR`
//!   (~/isoloom-check-all), `ISOLOOM_LINUX_JOBS` build jobs.
//! - `windows`: a Windows host over SSH, MSVC toolchain: `ISOLOOM_WIN_HOST` (10.0.0.40),
//!   `ISOLOOM_WIN_USER` (florianamette), `ISOLOOM_WIN_PORT`, `ISOLOOM_WIN_DIR`
//!   (`C:/isoloom-check-all`), `ISOLOOM_WIN_JOBS`; `cargo xtask host-setup` prepares it.
//! - `linux-docker`: a pinned Rust container on this machine (`xtask/check-all/linux.Dockerfile`),
//!   with the repositories mounted at their own paths.
//!
//! Each checkout builds in its own folders on the other hosts, so a run never tests another
//! worktree's build ([`checkout`]): in the container, the Docker volumes
//! `isoloom-check-all-<repo>-<key>-target` (and the launcher's `...-node-modules`), the key from
//! the checkout's path; on the SSH hosts, `<dir>/<key>/`, the key from the run's checkouts. Only
//! the downloads are shared (`isoloom-check-all-cache`).
//!
//! The SSH hosts' settings come from the environment, else from the hosts file
//! (`~/.isoloom-lab/hosts.env`, or the one `ISOLOOM_HOSTS_ENV` names; see [`settings`]).
//!
//! Isothrum and the launcher pin Isoloom and Isothrum commits by git, which GitHub hasn't got
//! (local `main` isn't pushed): before a run that copies either, each SSH host's mirrors of this
//! machine's Isoloom and Isothrum `main` are brought up to date, in its home, and its git sends the
//! GitHub URLs there ([`mirrors`]).
//!
//! The other hosts run the platform gates (the tests, and the launcher's `pnpm check`) on a copy
//! of the working trees as git sees them: tracked and new files, without the ignored ones. A
//! consumer linked by `cargo xtask dev-link` stays linked in the copy. Each host runs its part
//! as `cargo xtask check-all --leg <host>`, which prints each gate as `@@begin` and `@@result`
//! lines ([`protocol`]); the remote hosts run in the background while this machine runs its own.
//! `CARGO_BUILD_JOBS` defaults to 4, on each host unless its own `_JOBS` setting says otherwise.
//!
//! Repositories: this Isoloom tree, and the Isothrum and launcher checkouts next to the main
//! Isoloom checkout (`ISOLOOM_ISOTHRUM_DIR`, `ISOLOOM_LAUNCHER_DIR`). The launcher needs Node 24:
//! `ISOLOOM_NODE_BIN` names its bin folder when the one on `PATH` is another version. A set
//! `CARGO_TARGET_DIR` gets one subfolder per repository, so they never share one.
//!
//! Isoloom and Isothrum run the same gates, each through its own xtask (Isothrum's is `cargo run
//! -p xtask --`). `--deep` adds the slow ones to this Mac's part: every fuzz target for a minute
//! (the `fuzz` task) and mutation testing (`mutants`: Isoloom's core crates, Isothrum's compiler
//! and runner executor).
//!
//! `--clean` removes the hosts' build folders and copies, every checkout's (the remote folders,
//! the Docker volumes and image). `--prune-volumes` removes only those of checkouts no longer on
//! this machine (a deleted worktree's): the Docker volumes (and the shared ones of earlier
//! versions), and the folders of the SSH hosts `--hosts` names.

mod checkout;
mod collect;
mod docker;
mod gates;
mod hosts;
mod mirrors;
mod pack;
mod protocol;
mod remote;
mod settings;
mod summary;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use crate::{Res, repo};

pub(crate) use gates::TAURI_PACKAGES;
pub(crate) use protocol::{GateResult, Status};

/// A repository check-all knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Repo {
    /// This repository.
    Isoloom,
    /// The runner and the lab-model compiler.
    Isothrum,
    /// The Cyber CTF launcher.
    Launcher,
}

impl Repo {
    /// Its name in the table and the logs.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Repo::Isoloom => "isoloom",
            Repo::Isothrum => "isothrum",
            Repo::Launcher => "launcher",
        }
    }

    /// Its folder name in the copies on the other hosts.
    pub(crate) fn folder(self) -> &'static str {
        match self {
            Repo::Launcher => "cyber-ctf-launcher",
            other => other.id(),
        }
    }
}

/// A host the gates run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Host {
    /// This Mac, natively.
    Mac,
    /// The Linux host, over SSH.
    Linux,
    /// The Windows host, over SSH.
    Windows,
    /// A Linux container on this machine.
    LinuxDocker,
}

impl Host {
    /// Its name in the table and the logs.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Host::Mac => "mac",
            Host::Linux => "linux",
            Host::Windows => "windows",
            Host::LinuxDocker => "linux-docker",
        }
    }

    /// Whether it runs in the background, over SSH.
    fn remote(self) -> bool {
        matches!(self, Host::Linux | Host::Windows)
    }
}

/// Which gates a host runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Profile {
    /// Every gate: format, lints, the supply chain, docs, the ratchet, the minimum Rust, the
    /// features, the fuzz crate's build, the tests.
    Full,
    /// Every gate, then fuzzing and mutation testing.
    Deep,
    /// The gates whose result depends on the platform: the tests.
    Platform,
}

impl Profile {
    /// Whether it runs the gates that read the sources (format, lints, docs...), not only the
    /// tests.
    pub(crate) fn statics(self) -> bool {
        self != Profile::Platform
    }
}

/// The arguments of `cargo xtask check-all`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// The repositories, comma separated.
    #[arg(long, value_enum, value_delimiter = ',', default_value = "isoloom,isothrum,launcher")]
    repos: Vec<Repo>,
    /// The hosts, comma separated: mac, linux, windows, linux-docker.
    #[arg(long, value_enum, value_delimiter = ',', default_value = "mac")]
    hosts: Vec<Host>,
    /// Where the logs go (default: a new folder under the system's temporary folder).
    #[arg(long, value_name = "DIR")]
    logs: Option<PathBuf>,
    /// Also run the slow gates on this Mac: fuzzing and mutation testing.
    #[arg(long)]
    deep: bool,
    /// Remove the hosts' build folders and copies instead.
    #[arg(long)]
    clean: bool,
    /// Remove the build volumes (and the SSH hosts' folders, for those --hosts names) of the
    /// checkouts no longer on this machine instead.
    #[arg(long, conflicts_with = "clean")]
    prune_volumes: bool,
    /// Run this host's part here and print it as `@@begin`/`@@result` lines (how the other hosts
    /// run theirs).
    #[arg(long, value_enum, hide = true)]
    leg: Option<Host>,
    /// The gates to run with --leg.
    #[arg(long, value_enum, hide = true, default_value = "platform")]
    profile: Profile,
    #[arg(long, hide = true)]
    isoloom: Option<PathBuf>,
    #[arg(long, hide = true)]
    isothrum: Option<PathBuf>,
    #[arg(long, hide = true)]
    launcher: Option<PathBuf>,
    /// The parent of each repository's build folder, with --leg.
    #[arg(long, hide = true)]
    target_root: Option<PathBuf>,
}

/// The three checkouts, as they are on disk (each one's build folders are keyed by its path).
#[derive(Debug, Clone)]
pub(crate) struct Dirs {
    /// This Isoloom tree.
    pub isoloom: PathBuf,
    /// Isothrum's checkout.
    pub isothrum: PathBuf,
    /// The launcher's checkout.
    pub launcher: PathBuf,
}

impl Dirs {
    /// The checkout of `repo`.
    pub(crate) fn of(&self, repo: Repo) -> &Path {
        match repo {
            Repo::Isoloom => &self.isoloom,
            Repo::Isothrum => &self.isothrum,
            Repo::Launcher => &self.launcher,
        }
    }
}

/// What every host's part shares: the repositories, where they are, and the logs folder.
pub(crate) struct Run {
    /// The repositories asked for.
    pub repos: Vec<Repo>,
    /// Their checkouts.
    pub dirs: Dirs,
    /// Where each host's and gate's log goes.
    pub logs: PathBuf,
    /// Whether this Mac also runs the slow gates.
    pub deep: bool,
    /// The hosts reached over SSH.
    pub ssh: settings::Hosts,
}

/// Runs the gates (or, with `--leg`, this host's part; with `--clean` or `--prune-volumes`, the
/// clean-up).
pub(crate) fn run(a: &Args) -> Res<ExitCode> {
    let root = repo::root();
    let code = repo::code_dir(&root);
    let dirs = Dirs {
        isoloom: a.isoloom.clone().unwrap_or_else(|| repo::physical(&root)),
        isothrum: a
            .isothrum
            .clone()
            .unwrap_or_else(|| repo::physical(&repo::consumer("ISOLOOM_ISOTHRUM_DIR", &code, "isothrum"))),
        launcher: a
            .launcher
            .clone()
            .unwrap_or_else(|| repo::physical(&repo::consumer("ISOLOOM_LAUNCHER_DIR", &code, "cyber-ctf-launcher"))),
    };
    if let Some(host) = a.leg {
        gates::leg(host, a.profile, &dirs, a.target_root.as_deref()).run(&a.repos, &mut collect::Printer);
        return Ok(ExitCode::SUCCESS);
    }
    let ssh = settings::load()?;
    if a.clean {
        hosts::clean(&a.hosts, &dirs, &ssh);
        return Ok(ExitCode::SUCCESS);
    }
    if a.prune_volumes {
        hosts::prune(&a.hosts, &ssh);
        return Ok(ExitCode::SUCCESS);
    }
    let logs = a.logs.clone().unwrap_or_else(default_logs);
    std::fs::create_dir_all(&logs)?;
    let run = Run {
        repos: a.repos.clone(),
        dirs,
        logs,
        deep: a.deep,
        ssh,
    };
    let names = |ids: Vec<&str>| ids.join(",");
    println!(
        "check-all: {} on {}",
        names(run.repos.iter().map(|r| r.id()).collect()),
        names(a.hosts.iter().map(|h| h.id()).collect())
    );
    for r in [Repo::Isoloom, Repo::Isothrum, Repo::Launcher] {
        println!("  {:<8} {}", r.id(), run.dirs.of(r).display());
    }
    for (host, remote) in [(Host::Linux, &run.ssh.linux), (Host::Windows, &run.ssh.windows)] {
        if a.hosts.contains(&host) {
            let port = remote.port.map(|p| format!(":{p}")).unwrap_or_default();
            println!("  {:<8} {}{port}, {} build jobs", host.id(), remote.target(), remote.jobs);
        }
    }
    let runs = all_hosts(&a.hosts, &run);
    print!("{}", summary::table(&runs, &run.logs));
    print!("{}", summary::failure_tails(&runs, &run.logs));
    Ok(if summary::any_failed(&runs) { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

/// A new logs folder under the system's temporary folder.
fn default_logs() -> PathBuf {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    std::env::temp_dir().join("isoloom-check-all").join(format!("run-{secs}"))
}

/// Every host's part: the remote ones in the background, this machine's one after the other.
fn all_hosts(hosts: &[Host], run: &Run) -> Vec<summary::HostRun> {
    std::thread::scope(|s| {
        let remote: Vec<_> = hosts.iter().filter(|h| h.remote()).map(|h| s.spawn(move || timed(*h, run))).collect();
        let mut runs: Vec<summary::HostRun> = hosts.iter().filter(|h| !h.remote()).map(|h| timed(*h, run)).collect();
        for (handle, host) in remote.into_iter().zip(hosts.iter().filter(|h| h.remote())) {
            runs.push(handle.join().unwrap_or_else(|_| summary::HostRun::lost(*host)));
        }
        runs.sort_by_key(|r| hosts.iter().position(|h| *h == r.host));
        runs
    })
}

/// One host's part, timed; a host that stopped before its first result gets a row saying so.
fn timed(host: Host, run: &Run) -> summary::HostRun {
    let start = Instant::now();
    let mut out = collect::Collector::new(host, &run.logs);
    hosts::run_host(host, run, &mut out);
    if out.results.is_empty() {
        out.host_failed(Status::Fail, &format!("no result (see {}.log)", host.id()));
    }
    summary::HostRun {
        host,
        results: out.results,
        secs: start.elapsed().as_secs(),
    }
}
