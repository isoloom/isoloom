//! Where each host's part runs: here (mac), over SSH (linux and windows, in [`remote`]), or in a
//! container on this machine (linux-docker, in [`docker`]); and how `--clean` and
//! `--prune-volumes` clear each.

use super::collect::Collector;
use super::{Dirs, Host, Profile, Run, Status, docker, gates, remote, settings};

/// Runs `host`'s part, its results into `out`.
pub(super) fn run_host(host: Host, run: &Run, out: &mut Collector) {
    match host {
        Host::Mac => mac(run, out),
        Host::Linux => remote::linux(run, out),
        Host::Windows => remote::windows(run, out),
        Host::LinuxDocker => docker::linux_docker(run, out),
    }
}

fn mac(run: &Run, out: &mut Collector) {
    if std::env::consts::OS != "macos" {
        return out.host_failed(Status::Skip, "not a Mac");
    }
    let profile = if run.deep { Profile::Deep } else { Profile::Full };
    gates::leg(Host::Mac, profile, &run.dirs, None).run(&run.repos, out);
}

/// The repositories as a `--repos` value.
pub(super) fn repos_arg(run: &Run) -> String {
    run.repos.iter().map(|r| r.id()).collect::<Vec<_>>().join(",")
}

/// Removes the hosts' build folders and copies.
pub(super) fn clean(hosts: &[Host], dirs: &Dirs, ssh: &settings::Hosts) {
    for host in hosts {
        let done = match host {
            Host::Linux => remote::clean_linux(&ssh.linux),
            Host::Windows => remote::clean_windows(&ssh.windows),
            Host::LinuxDocker => docker::clean(),
            Host::Mac => Ok(format!("the repositories' own build folders stay (cargo clean in {})", dirs.isoloom.display())),
        };
        match done {
            Ok(what) => println!("{}: {what}", host.id()),
            Err(e) => println!("{}: {e}", host.id()),
        }
    }
}

/// Removes the build folders of checkouts no longer on this machine: the Docker volumes always,
/// and the folders of the SSH hosts in `hosts`.
pub(super) fn prune(hosts: &[Host], ssh: &settings::Hosts) {
    let mut done = vec![(Host::LinuxDocker, docker::prune())];
    if hosts.contains(&Host::Linux) {
        done.push((Host::Linux, remote::prune_linux(&ssh.linux)));
    }
    if hosts.contains(&Host::Windows) {
        done.push((Host::Windows, remote::prune_windows(&ssh.windows)));
    }
    for (host, result) in done {
        match result {
            Ok(what) => println!("{}: {what}", host.id()),
            Err(e) => println!("{}: {e}", host.id()),
        }
    }
}
