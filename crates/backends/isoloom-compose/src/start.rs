//! How to start a Compose file: `wait` for everything but its leaf jobs (`up -d --wait`), then
//! run each of those jobs attached, in order, failing on its exit code. Computed from the typed
//! file this backend renders, for the run manifests' start steps and, as one shell line
//! ([`StartPlan::command`]), the start written atop the file and the hybrid Docker host's.

use crate::file::ComposeFile;

/// How to start a Compose file: the services to wait for, then the jobs to run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct StartPlan {
    /// Services to bring up with `docker compose up -d --wait` (empty: all of them).
    pub(crate) wait: Vec<String>,
    /// One-shot jobs nothing running waits for, to run after `wait`, in order.
    pub(crate) jobs: Vec<String>,
}

/// A service as the start plan reads it: its name, what it waits for, whether it runs once
/// (`restart: "no"`). Services in a profile aren't started, so they aren't read.
struct Started {
    name: String,
    deps: Vec<String>,
    one_shot: bool,
}

impl StartPlan {
    /// The plan of a Compose file: one-shots (`restart: "no"`) outside profiles that no other
    /// started service depends on are jobs; when there are none, `wait` is empty (start all).
    pub(crate) fn of(file: &ComposeFile) -> StartPlan {
        let started: Vec<Started> = file
            .services
            .iter()
            .filter(|(_, s)| s.profiles.is_empty())
            .map(|(name, s)| Started {
                name: name.clone(),
                deps: s.depends_on.keys().cloned().collect(),
                one_shot: s.runs_once(),
            })
            .collect();
        plan_of(&started)
    }

    /// The shell line that starts the file by hand with `compose` (e.g. `docker compose -f x`):
    /// everything but the jobs up, built and waited for (at most `wait_timeout` seconds when
    /// given), then each job attached, its exit code failing the line.
    pub(crate) fn command(&self, compose: &str, wait_timeout: Option<u32>) -> String {
        let timeout = wait_timeout.map(|t| format!(" --wait-timeout {t}")).unwrap_or_default();
        let services = self.wait.iter().map(|s| format!(" {s}")).collect::<Vec<_>>().concat();
        let jobs = self
            .jobs
            .iter()
            .map(|j| format!(" && {compose} up --no-deps --exit-code-from {j} {j}"))
            .collect::<Vec<_>>()
            .concat();
        format!("{compose} up -d --build --wait{timeout}{services}{jobs}")
    }
}

fn plan_of(started: &[Started]) -> StartPlan {
    // What the long-running services need, through chains of one-shots: `--wait` accepts those
    // exiting. The other one-shots are the jobs.
    let mut needed: Vec<&str> = Vec::new();
    let mut todo: Vec<&str> = started.iter().filter(|s| !s.one_shot).flat_map(|s| s.deps.iter().map(String::as_str)).collect();
    while let Some(n) = todo.pop() {
        if needed.contains(&n) {
            continue;
        }
        if let Some(s) = started.iter().find(|s| s.name == n) {
            todo.extend(s.deps.iter().map(String::as_str));
        }
        needed.push(n);
    }
    let jobs: Vec<String> = started
        .iter()
        .filter(|s| s.one_shot && !needed.contains(&s.name.as_str()))
        .map(|s| s.name.clone())
        .collect();
    if jobs.is_empty() {
        return StartPlan::default();
    }
    let wait = started.iter().map(|s| s.name.clone()).filter(|n| !jobs.contains(n)).collect();
    StartPlan { wait, jobs }
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;
    use crate::file::{Condition, Restart, Service};

    /// A service that runs once (`restart: "no"`) or keeps running, in `profiles`, after `deps`.
    fn service(once: bool, profiles: &[&str], deps: &[&str]) -> Service {
        let mut s = Service {
            restart: Some(if once { Restart::No } else { Restart::UnlessStopped }),
            profiles: profiles.iter().map(|p| (*p).to_owned()).collect(),
            ..Service::default()
        };
        for d in deps {
            s.after(*d, Condition::Started);
        }
        s
    }

    fn file(services: &[(&str, Service)]) -> ComposeFile {
        let mut f = ComposeFile::default();
        f.services.extend(services.iter().map(|(n, s)| ((*n).to_owned(), s.clone())));
        f
    }

    #[test]
    fn a_job_nothing_waits_for_runs_after_the_rest_is_up() {
        let plan = StartPlan::of(&file(&[
            ("web", service(false, &[], &["db", "isoloom-init-web-1"])),
            ("isoloom-init-web-1", service(true, &[], &[])),
            ("db", service(false, &[], &[])),
            ("seed", service(true, &[], &["db"])),
            ("isoloom-check", service(true, &["check"], &[])),
        ]));
        assert_eq!(plan.jobs, ["seed"]);
        assert_eq!(plan.wait, ["web", "isoloom-init-web-1", "db"]);
    }

    #[test]
    fn without_leaf_jobs_everything_is_waited_for() {
        assert_eq!(StartPlan::of(&file(&[("web", service(false, &[], &[]))])), StartPlan::default());
        assert_eq!(StartPlan::of(&ComposeFile::default()), StartPlan::default());
        assert_eq!(StartPlan::default().command("dc", Some(60)), "dc up -d --build --wait --wait-timeout 60");
    }

    #[test]
    fn the_command_waits_for_the_rest_then_runs_each_job_attached() {
        let plan = StartPlan {
            wait: vec!["web".into(), "db".into()],
            jobs: vec!["seed".into(), "warm".into()],
        };
        assert_eq!(
            plan.command("dc", None),
            "dc up -d --build --wait web db && dc up --no-deps --exit-code-from seed seed && dc up --no-deps --exit-code-from warm warm"
        );
    }

    #[test]
    fn a_rendered_file_waits_for_what_runs_and_runs_its_leaf_init_jobs_after() {
        // The store's init job is waited for by the app; the app's by nothing.
        let env = crate::testing::environment(
            "version: 1\nname: leaves\nnetworks: { net: { cidr: 10.71.0.0/24 } }\nmachines:\n  store: { networks: { net: 5 }, app: { image: redis }, setup: [fill.sh] }\n  app: { networks: { net: 6 }, depends_on: [store], app: { image: nginx }, setup: [prime.sh] }\n",
        );
        let rendered = crate::render::compose_file(Target::Docker, &env).expect("renders");
        let plan = StartPlan::of(&rendered.file);
        assert_eq!(plan.jobs, ["isoloom-init-app-1"]);
        assert!(
            plan.wait.contains(&"app".to_owned()) && plan.wait.contains(&"isoloom-init-store-1".to_owned()),
            "{plan:?}"
        );
        assert!(!plan.wait.iter().any(|w| w.starts_with("isoloom-check")), "profile services aren't started");
    }
}
