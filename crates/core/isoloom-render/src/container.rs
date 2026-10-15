//! What every target that runs the machines as containers shares, whichever backend writes the
//! files (Compose, Kubernetes) or builds the host they run on (a cloud VM, a local VM): the
//! images Isoloom brings, the readiness probe, the one-shot jobs and how a Compose project is
//! started, the project files read into Compose's environment, and the containers' side of a
//! hybrid environment. Pure functions over the spec and the plan.

use std::fmt::Write as _;

use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::model::{Machine, MachineFile, Protocol, Spec};
use isoloom_spec::shell::quote;

use crate::GeneratedFile;

/// The size of the VM the containers run on (a cloud VM, a local VM): every container's at
/// once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct HostSize {
    /// CPUs: one per container unless it says, at least 2, at most 16.
    pub cpus: u32,
    /// Memory in MB: 512 per container unless it says, plus 1 GB for Docker itself.
    pub memory_mb: u32,
    /// Disk in GB: 5 per container unless it says, at least 20.
    pub disk_gb: u32,
}

impl HostSize {
    /// The host's size for the environment's containers.
    #[must_use]
    pub fn of(env: &Environment) -> HostSize {
        let containers = env.machines.values().filter(|m| m.shape == MachineShape::Container);
        let (cpus, memory_mb, disk_gb) = containers.fold((0u32, 1024u32, 0u32), |(c, m, d), x| {
            (
                c.saturating_add(x.size.cpus.unwrap_or(1)),
                m.saturating_add(x.size.memory_mb.unwrap_or(512)),
                d.saturating_add(x.size.disk_gb.unwrap_or(5)),
            )
        });
        HostSize {
            cpus: cpus.clamp(2, 16),
            memory_mb,
            disk_gb: disk_gb.max(20),
        }
    }

    /// One of three sizes of VM by its memory: up to 4 GB, up to 8 GB, more.
    #[must_use]
    pub fn pick<T: Copy>(&self, [small, medium, large]: [T; 3]) -> T {
        match self.memory_mb {
            0..=4096 => small,
            4097..=8192 => medium,
            _ => large,
        }
    }
}

/// The check runner: a small image with `sh`, `curl` and busybox `nc`.
pub const CHECK_IMAGE: &str = "curlimages/curl:8.11.1";

/// A static busybox Isoloom brings into every machine with services, so its probe needs
/// nothing from the machine's image (distroless and `scratch` images have no shell).
pub const PROBE_IMAGE: &str = "busybox:1.37.0-musl";
/// Where a machine sees that busybox (read-only).
pub const PROBE_DIR: &str = "/.isoloom-probe";

/// A small image with busybox `ip`, for the router, route sidecars and the stand-in.
pub const UTILITY_IMAGE: &str = "alpine:3.20";

/// A TCP probe for each service port but the dormant ones, run by Isoloom's own busybox (see
/// [`PROBE_IMAGE`]).
pub fn probe(m: &Machine) -> String {
    m.ready_ports()
        .iter()
        .map(|port| format!("{PROBE_DIR}/busybox nc -z -w 2 127.0.0.1 {port}"))
        .collect::<Vec<_>>()
        .join(" && ")
}

/// Everything a machine's readiness checks, as one line for Isoloom's busybox `sh`: its ports
/// ([`probe`]), then its own healthcheck (`health`; a page through busybox `wget`), grouped so
/// its own `||` stays its own. Empty when there is neither.
pub fn ready_test(m: &Machine, health: Option<&isoloom_plan::Health>) -> String {
    let own = health.map(|h| format!("( {} )", h.shell_test(&format!("{PROBE_DIR}/busybox wget"))));
    [probe(m)].into_iter().filter(|t| !t.is_empty()).chain(own).collect::<Vec<_>>().join(" && ")
}

/// The runner of a machine's `exec` checks, in `.isoloom/docker/checks/`: piped into the
/// machine (`docker compose exec -T <machine> sh -s < exec-<machine>.sh`).
pub fn exec_runner(machine: &str) -> String {
    format!("exec-{machine}.sh")
}

/// The one-shot `init:` jobs of a machine, in order: `<machine>-init-<n>`.
pub fn init_names(name: &str, m: &Machine) -> Vec<String> {
    let n = m.docker.as_ref().map_or(0, |d| d.init.len());
    (1..=n).map(|i| format!("{name}-init-{i}")).collect()
}

/// The one-shot `init:` jobs no running service waits for: those of machines nothing depends
/// on. Compose's `up --wait` fails when such a job exits, even with 0, so starters bring the
/// rest up with `--wait` first, then run these attached, in order (see [`start_commands`]).
pub fn leaf_jobs(spec: &Spec) -> Vec<String> {
    spec.machines
        .iter()
        .filter(|(name, m)| m.docker.is_some() && !spec.machines.values().any(|o| o.docker.is_some() && o.depends_on.iter().any(|d| d == *name)))
        .flat_map(|(name, m)| init_names(name, m))
        .collect()
}

/// The shell command that starts a Compose project with `compose` (e.g. `docker compose -f x`):
/// plain `up -d --build --wait` when no job is left over, else everything but the jobs up and
/// waited for, then each job attached. `jobs` are the leaf jobs ([`leaf_jobs`]); the services
/// to wait for are listed at run time.
pub fn start_commands(compose: &str, jobs: &[String], wait_timeout: Option<u32>) -> String {
    let timeout = wait_timeout.map(|t| format!(" --wait-timeout {t}")).unwrap_or_default();
    if jobs.is_empty() {
        return format!("{compose} up -d --build --wait{timeout}");
    }
    let exclude = jobs.iter().fold(String::new(), |mut out, j| {
        let _ = write!(out, " -e {j}");
        out
    });
    let mut cmd = format!("{compose} up -d --build --wait{timeout} $({compose} config --services | grep -vx{exclude})");
    for j in jobs {
        let _ = write!(cmd, " && {compose} up --no-deps --exit-code-from {j} {j}");
    }
    cmd
}

/// The variable of Compose's own environment holding machine `machine`'s `n`th file, when it
/// is a project file with an owner or a mode (`ISOLOOM_FILE_WEB_1`). Machine names are
/// kebab-case, so two files never share one.
pub fn variable(machine: &str, n: usize) -> String {
    format!("ISOLOOM_FILE_{}_{n}", machine.to_ascii_uppercase().replace('-', "_"))
}

/// Whether file `f` is copied in from Compose's environment: a project file the spec gives an
/// owner or a mode (outside Swarm, Compose ignores both for a `file` config).
pub fn copied_from_project(f: &MachineFile) -> bool {
    f.source.is_some() && f.sets_owner_or_mode()
}

/// The project files Compose reads from its own environment, as (variable, path in the
/// project): what whoever starts Compose sets first (`isoloom run`, or a Docker host's start
/// script).
pub fn project_variables(env: &Environment) -> Vec<(String, String)> {
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Container)
        .flat_map(|(machine, mp)| {
            mp.files
                .iter()
                .enumerate()
                .filter(|(_, f)| copied_from_project(f))
                .filter_map(move |(n, f)| Some((variable(machine, n), f.source.clone()?)))
        })
        .collect()
}

/// On a Docker host, in the project's folder before `docker compose up`: each project file with
/// an owner or a mode read into its variable, its last line breaks kept, as commands joined by
/// `&&` (ending with one). Nothing when there are none.
pub fn host_environment(env: &Environment) -> String {
    let mut out = String::new();
    for (var, path) in project_variables(env) {
        let _ = write!(out, "{var}=\"$(cat {}; printf x)\" && {var}=\"${{{var}%x}}\" && export {var} && ", quote(&path));
    }
    out
}

/// The containers' side of a hybrid environment, as the Compose backend writes it for the
/// Docker host VM the Vagrant backend adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridContainers {
    /// `.isoloom/hybrid/compose.yml`.
    pub file: GeneratedFile,
    /// The ports published on the host, in the order of the services: the Docker host forwards
    /// them.
    pub published: Vec<(u16, Protocol)>,
    /// The networks containers are on, in spec order: the Docker host creates them.
    pub networks: Vec<String>,
}

/// The Docker network a hybrid environment's container network is on the host: named after the
/// environment.
pub fn docker_net(env: &Environment, net: &str) -> String {
    format!("isoloom-{}-{net}", env.name)
}

#[cfg(test)]
mod tests {
    use isoloom_plan::Environment;
    use isoloom_spec::model::Target;

    use super::HostSize;

    /// The containers' host for `yaml`, on `docker-vm`.
    fn host_size(yaml: &str) -> HostSize {
        let lab = isoloom_plan::Lab::parse(yaml).expect("parses");
        HostSize::of(&Environment::build(&lab.for_target(Target::DockerVm), Target::DockerVm, None))
    }

    #[test]
    fn every_container_counts_and_docker_takes_a_gigabyte() {
        let size = host_size(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { networks: { lan: 10 }, app: { image: nginx } }\n  b: { networks: { lan: 11 }, resources: { cpus: 3, memory_mb: 4096, disk_gb: 30 }, app: { image: nginx } }\n",
        );
        assert_eq!(
            size,
            HostSize {
                cpus: 4,
                memory_mb: 512 + 4096 + 1024,
                disk_gb: 35
            }
        );
        assert_eq!(size.pick(["small", "medium", "large"]), "medium");
    }

    #[test]
    fn a_few_small_containers_take_the_smallest_host() {
        let size =
            host_size("version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { networks: { lan: 10 }, app: { image: nginx } }\n");
        assert_eq!((size.cpus, size.memory_mb, size.disk_gb), (2, 1536, 20));
        assert_eq!(size.pick(["small", "medium", "large"]), "small");
    }
}
