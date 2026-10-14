//! What a machine runs and when it is healthy, resolved once for every target: its command,
//! the directory it runs in, the steps before it, and its healthcheck with every pace in
//! seconds (Kubernetes counts in seconds; Compose and the VM scripts are given the same).

use isoloom_spec::model::{CommandLine, Healthcheck, Machine, seconds};

/// The healthcheck's pace when the spec leaves it out: the port probe's own.
pub const DEFAULT_INTERVAL: u32 = 5;
/// How long one try may take, by default.
pub const DEFAULT_TIMEOUT: u32 = 3;
/// Failed tries in a row before the machine is unhealthy, by default (5 minutes at 5s).
pub const DEFAULT_RETRIES: u32 = 60;
/// The grace period after start, by default.
pub const DEFAULT_START_PERIOD: u32 = 10;

/// A machine's own process. Empty when the spec sets none of it: the image's or the VM's own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Process {
    /// Its own command, instead of the image's.
    pub command: Option<CommandLine>,
    /// Where the command, the `before` steps and the healthcheck run.
    pub working_dir: Option<String>,
    /// The scripts run before the command at every start, from the project folder.
    pub before: Vec<String>,
    /// When it is healthy, beside its services' ports answering.
    pub health: Option<Health>,
}

impl Process {
    /// The process machine `m` declares.
    pub fn of(m: &Machine) -> Process {
        Process {
            command: m.command.clone(),
            working_dir: m.working_dir.clone(),
            before: m.before.clone(),
            health: m.healthcheck.as_ref().and_then(Health::of),
        }
    }

    /// Whether the spec sets any of it.
    pub fn is_empty(&self) -> bool {
        self.command.is_none() && self.working_dir.is_none() && self.before.is_empty() && self.health.is_none()
    }
}

/// A healthcheck, its pace in seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Health {
    /// What is tried.
    pub test: HealthTest,
    /// Seconds between two tries.
    pub interval: u32,
    /// Seconds one try may take.
    pub timeout: u32,
    /// Failed tries in a row before the machine is unhealthy.
    pub retries: u32,
    /// Seconds after start before a failed try counts.
    pub start_period: u32,
}

/// What a healthcheck tries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthTest {
    /// A command run inside the machine, healthy when it exits with 0.
    Command(CommandLine),
    /// A page on the machine's own port, healthy when it answers with a 2xx or 3xx status.
    Http {
        /// The port.
        port: u16,
        /// The page's path, from `/`.
        path: String,
    },
}

impl Health {
    /// The plan of a validated healthcheck; `None` when it says nothing to try.
    pub fn of(h: &Healthcheck) -> Option<Health> {
        let test = match (&h.test, &h.http) {
            (Some(t), _) => HealthTest::Command(t.clone()),
            (None, Some(http)) => HealthTest::Http {
                port: http.port,
                path: http.path.clone(),
            },
            (None, None) => return None,
        };
        let pace = |v: &Option<String>, default| v.as_deref().and_then(seconds).unwrap_or(default);
        Some(Health {
            test,
            interval: pace(&h.interval, DEFAULT_INTERVAL),
            timeout: pace(&h.timeout, DEFAULT_TIMEOUT),
            retries: h.retries.unwrap_or(DEFAULT_RETRIES),
            start_period: pace(&h.start_period, DEFAULT_START_PERIOD),
        })
    }

    /// The test as one line of shell, run inside the machine: the command as written, or the
    /// page fetched with `wget` (busybox has it, as Isoloom's probe does on the containers).
    /// `wget`'s own failure on a 4xx or 5xx status is the unhealthy answer.
    pub fn shell_test(&self, wget: &str) -> String {
        match &self.test {
            HealthTest::Command(c) => c.shell_line(),
            HealthTest::Http { port, path } => {
                format!("{wget} -q -O /dev/null -T {} http://127.0.0.1:{port}{path}", self.timeout)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::{CommandLine, Healthcheck, HttpCheck};

    use super::{Health, HealthTest};

    #[test]
    fn the_pace_defaults_to_the_port_probes() {
        let h = Healthcheck {
            test: None,
            http: Some(HttpCheck {
                port: 8080,
                path: "/healthz".into(),
            }),
            interval: Some("1m".into()),
            timeout: None,
            retries: None,
            start_period: Some("0s".into()),
        };
        let health = Health::of(&h).expect("a test");
        assert_eq!((health.interval, health.timeout, health.retries, health.start_period), (60, 3, 60, 0));
        assert_eq!(
            health.test,
            HealthTest::Http {
                port: 8080,
                path: "/healthz".into()
            }
        );
        assert_eq!(health.shell_test("wget"), "wget -q -O /dev/null -T 3 http://127.0.0.1:8080/healthz");
        let cmd = Health {
            test: HealthTest::Command(CommandLine::Exec(vec!["pg_isready".into()])),
            ..health
        };
        assert_eq!(cmd.shell_test("wget"), "'pg_isready'");
    }
}
