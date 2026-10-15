//! A service of the Compose file: what it runs, how, on which networks, with what, and when it
//! starts.

use std::fmt;
use std::ops::Not;

use indexmap::IndexMap;
use serde::{Serialize, Serializer};

use super::{Attachment, ConfigMount, Mount, Reservations, ServiceDevice, ServiceSecret};

/// One service.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Service {
    /// Builds the image from a context instead of pulling it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) build: Option<Build>,
    /// The image to run (pulled when it isn't built).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) image: Option<String>,
    /// The platform to pull or build for (`linux/arm64`), when not the host's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) platform: Option<String>,
    /// Profiles the service belongs to: it starts only when one is enabled.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) profiles: Vec<String>,
    /// Runs with every capability and the host's devices.
    #[serde(skip_serializing_if = "Not::not")]
    pub(crate) privileged: bool,
    /// Mounts the container's root file system read-only.
    #[serde(skip_serializing_if = "Not::not")]
    pub(crate) read_only: bool,
    /// The user the command runs as.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) user: Option<String>,
    /// Kernel parameters set in the container's namespaces.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) sysctls: IndexMap<String, String>,
    /// Resource limits for the container's processes, by name (`nofile`).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) ulimits: IndexMap<String, Ulimit>,
    /// The size of `/dev/shm`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) shm_size: Option<String>,
    /// Paths mounted as in-memory file systems.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tmpfs: Vec<String>,
    /// The resolvers the container uses.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) dns: Vec<String>,
    /// The container resolver's options (`ndots:2`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) dns_opt: Vec<String>,
    /// The search domains the container's resolver appends.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) dns_search: Vec<String>,
    /// The container's domain name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) domainname: Option<String>,
    /// Replaces the image's entrypoint.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) entrypoint: Vec<String>,
    /// Replaces the image's command.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) command: Vec<String>,
    /// The directory the process starts in, instead of the image's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) working_dir: Option<String>,
    /// The container's host name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hostname: Option<String>,
    /// No network, or another container's network stack, instead of joining networks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) network_mode: Option<NetworkMode>,
    /// `None` for a plain attachment (`net:` with no settings).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) networks: IndexMap<String, Option<Attachment>>,
    /// Kernel capabilities added to the container's defaults.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) cap_add: Vec<String>,
    /// The host's devices, at a path inside the container.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) devices: Vec<ServiceDevice>,
    /// Lines added to the container's `/etc/hosts` (`name:address`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) extra_hosts: Vec<String>,
    /// Environment variables; `Some` of an empty map still writes the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<IndexMap<String, String>>,
    /// Ports published on the host (`address:host:container`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) ports: Vec<String>,
    /// Mounts: bind mounts and named volumes (`source:target[:ro]`), and the mounts with
    /// options in the long form.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) volumes: Vec<Mount>,
    /// The secrets it gets, as files.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) secrets: Vec<ServiceSecret>,
    /// The configs placed in the container, each at its path.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) configs: Vec<ConfigMount>,
    /// How Docker decides the container is healthy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) healthcheck: Option<Healthcheck>,
    /// Services started first, each with the condition it must reach.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) depends_on: IndexMap<String, Dependency>,
    /// Resource limits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) deploy: Option<Deploy>,
    /// The container's labels (Isoloom's own, to find what it started).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) labels: IndexMap<String, String>,
    /// When Docker restarts the container.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) restart: Option<Restart>,
}

impl Service {
    /// Waits for `service` to reach `condition` before starting.
    pub(crate) fn after(&mut self, service: impl Into<String>, condition: Condition) {
        self.depends_on.insert(service.into(), Dependency { condition });
    }

    /// Whether it runs once and exits (`restart: "no"`): a job, not a long-running service.
    pub(crate) fn runs_once(&self) -> bool {
        self.restart == Some(Restart::No)
    }
}

/// A service's `build`.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Build {
    /// The build context, relative to the Compose file.
    pub(crate) context: String,
    /// The Dockerfile, when not `Dockerfile` in the context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dockerfile: Option<String>,
    /// The stage of a multi-stage Dockerfile to build, when not its last.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    /// Build arguments.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) args: IndexMap<String, String>,
    /// Labels on the built image.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) labels: IndexMap<String, String>,
}

/// A ulimit: one number when soft and hard agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub(crate) enum Ulimit {
    /// Soft and hard alike.
    Both(u64),
    /// Each its own.
    Range {
        /// The limit a process starts with, and may raise up to `hard`.
        soft: u64,
        /// The ceiling only a privileged process may raise.
        hard: u64,
    },
}

impl Ulimit {
    /// The ulimit of `soft` and `hard`: one number when they agree.
    pub(crate) fn new(soft: u64, hard: u64) -> Ulimit {
        if soft == hard { Ulimit::Both(soft) } else { Ulimit::Range { soft, hard } }
    }
}

/// A whole number of seconds, written as Compose reads a duration (`3s`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Seconds(pub(crate) u32);

impl fmt::Display for Seconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}s", self.0)
    }
}

impl Serialize for Seconds {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// A healthcheck.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Healthcheck {
    /// The test command (`CMD-SHELL` and a shell line).
    pub(crate) test: Vec<String>,
    /// The time between two tests.
    pub(crate) interval: Seconds,
    /// How long a test may run before it counts as failed.
    pub(crate) timeout: Seconds,
    /// Failed tests in a row before the container is unhealthy.
    pub(crate) retries: u32,
    /// A grace period after start when failures don't count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) start_period: Option<Seconds>,
}

impl Healthcheck {
    /// A shell test, retried every `every` (also its timeout) up to `retries` times.
    pub(crate) fn shell(test: impl Into<String>, every: Seconds, retries: u32) -> Healthcheck {
        Healthcheck {
            test: vec!["CMD-SHELL".into(), test.into()],
            interval: every,
            timeout: every,
            retries,
            start_period: None,
        }
    }
}

/// A `depends_on` entry: the condition the dependency must reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct Dependency {
    /// What the dependency must reach.
    pub(crate) condition: Condition,
}

/// What a service waits for of one it depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum Condition {
    /// Its container started.
    #[serde(rename = "service_started")]
    Started,
    /// Its healthcheck passes.
    #[serde(rename = "service_healthy")]
    Healthy,
    /// It ran once and exited 0.
    #[serde(rename = "service_completed_successfully")]
    CompletedSuccessfully,
}

/// When Docker restarts a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Restart {
    /// Never: a job that runs once.
    No,
    /// Always, until it is stopped on purpose.
    UnlessStopped,
}

/// A service's network stack when it doesn't join networks of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NetworkMode {
    /// No network at all.
    None,
    /// Another service's (`service:<name>`): its addresses, names and routes.
    Service(String),
}

impl fmt::Display for NetworkMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkMode::None => f.write_str("none"),
            NetworkMode::Service(name) => write!(f, "service:{name}"),
        }
    }
}

impl Serialize for NetworkMode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// `deploy`.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Deploy {
    /// The container's resources.
    pub(crate) resources: Resources,
}

/// `deploy.resources`.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Resources {
    /// The most the container may use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) limits: Option<Limits>,
    /// What the container is given before it starts (its GPUs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reservations: Option<Reservations>,
}

/// `deploy.resources.limits`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Limits {
    /// CPUs, as a decimal number of cores.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cpus: Option<String>,
    /// Memory, with its unit (`512M`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) memory: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(value: &impl Serialize) -> String {
        serde_yaml_ng::to_string(value).expect("serializes")
    }

    #[test]
    fn a_job_never_restarts() {
        let job = Service {
            restart: Some(Restart::No),
            ..Service::default()
        };
        assert!(job.runs_once());
        assert_eq!(yaml(&job), "restart: no\n");
        assert_eq!(yaml(&Restart::UnlessStopped), "unless-stopped\n");
    }

    #[test]
    fn a_dependency_names_its_condition_as_compose_does() {
        let mut svc = Service::default();
        svc.after("db", Condition::Healthy);
        svc.after("seed", Condition::CompletedSuccessfully);
        assert_eq!(
            yaml(&svc),
            "depends_on:\n  db:\n    condition: service_healthy\n  seed:\n    condition: service_completed_successfully\n"
        );
    }

    #[test]
    fn a_network_mode_is_none_or_another_service() {
        assert_eq!(yaml(&NetworkMode::None), "none\n");
        assert_eq!(yaml(&NetworkMode::Service("web".into())), "service:web\n");
    }

    #[test]
    fn a_shell_healthcheck_times_out_at_its_interval() {
        let h = Healthcheck::shell("true", Seconds(3), 60);
        assert_eq!(yaml(&h), "test:\n- CMD-SHELL\n- 'true'\ninterval: 3s\ntimeout: 3s\nretries: 60\n");
    }

    #[test]
    fn a_ulimit_is_one_number_when_soft_and_hard_agree() {
        assert_eq!(Ulimit::new(5, 5), Ulimit::Both(5));
        assert_eq!(Ulimit::new(5, 9), Ulimit::Range { soft: 5, hard: 9 });
    }
}
