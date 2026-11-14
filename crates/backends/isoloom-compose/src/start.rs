//! How a Compose file starts: one `docker compose up -d --wait`. Compose's `--wait` waits for a
//! service to run (or to be healthy, with a healthcheck), and accepts a one-shot service's exit
//! only when another service waits for it to complete (`service_completed_successfully`): a
//! one-shot nothing waits for would fail `--wait` by exiting, even with 0. So the jobs nothing
//! running waits for (a machine's `setup` scripts, run once it is up) are waited for by one
//! small service of Isoloom's own, [`JOBS_DONE`], which starts once each of them exited 0: `up
//! --wait` then ends well only when every job did, and a job that fails fails it.

use indexmap::IndexMap;
use isoloom_plan::Environment;
use isoloom_render::container::PROBE_IMAGE;

use crate::file::{Condition, NetworkMode, Restart, Service};

/// The service that runs once every job nothing else waits for exited 0, under the prefix an
/// environment's machines can't take.
pub(crate) const JOBS_DONE: &str = "isoloom-jobs-done";

/// The line that starts the file with `compose` (e.g. `docker compose -f x`): everything built,
/// started and waited for, its jobs included (at most `wait_timeout` seconds when given).
pub(crate) fn command(compose: &str, wait_timeout: Option<u32>) -> String {
    let timeout = wait_timeout.map(|t| format!(" --wait-timeout {t}")).unwrap_or_default();
    format!("{compose} up -d --build --wait{timeout}")
}

/// [`JOBS_DONE`] added to `services` when some started one-shot (`restart: "no"`) service is a
/// job no long-running service waits for, even through other jobs; nothing otherwise. It waits
/// for each such job to exit 0, then does nothing until stopped (the probe's busybox, with no
/// network).
pub(crate) fn wait_for_jobs(env: &Environment, services: &mut IndexMap<String, Service>) {
    let jobs = leaf_jobs(services);
    if jobs.is_empty() {
        return;
    }
    let mut done = Service {
        image: Some(env.images.pull(PROBE_IMAGE)),
        entrypoint: vec!["sleep".into(), "2147483647".into()],
        // Stopped at once: the engine's init forwards the stop signal, which `sleep` as PID 1
        // would ignore.
        init: true,
        network_mode: Some(NetworkMode::None),
        restart: Some(Restart::UnlessStopped),
        ..Service::default()
    };
    for job in jobs {
        done.after(job, Condition::CompletedSuccessfully);
    }
    services.insert(JOBS_DONE.to_owned(), done);
}

/// The one-shots outside profiles (a profile's services don't start with `up`) that no
/// long-running service needs, through chains of one-shots, in the file's order.
fn leaf_jobs(services: &IndexMap<String, Service>) -> Vec<String> {
    let started: Vec<(&String, &Service)> = services.iter().filter(|(_, s)| s.profiles.is_empty()).collect();
    let mut needed: Vec<&str> = Vec::new();
    let mut todo: Vec<&str> = started
        .iter()
        .filter(|(_, s)| !s.runs_once())
        .flat_map(|(_, s)| s.depends_on.keys().map(String::as_str))
        .collect();
    while let Some(n) = todo.pop() {
        if needed.contains(&n) {
            continue;
        }
        if let Some(s) = services.get(n) {
            todo.extend(s.depends_on.keys().map(String::as_str));
        }
        needed.push(n);
    }
    started
        .iter()
        .filter(|(name, s)| s.runs_once() && !needed.contains(&name.as_str()))
        .map(|(name, _)| (*name).clone())
        .collect()
}

#[cfg(test)]
mod tests {
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

    fn services(list: &[(&str, Service)]) -> IndexMap<String, Service> {
        list.iter().map(|(n, s)| ((*n).to_owned(), s.clone())).collect()
    }

    #[test]
    fn a_job_nothing_waits_for_is_a_leaf_one_a_needed_one_and_a_profiles_are_not() {
        let jobs = leaf_jobs(&services(&[
            ("web", service(false, &[], &["db", "isoloom-init-web-1"])),
            ("isoloom-init-web-1", service(true, &[], &["isoloom-probe"])),
            ("isoloom-probe", service(true, &[], &[])),
            ("db", service(false, &[], &[])),
            ("seed", service(true, &[], &["db"])),
            ("isoloom-check", service(true, &["check"], &[])),
        ]));
        assert_eq!(jobs, ["seed"]);
        assert_eq!(leaf_jobs(&services(&[("web", service(false, &[], &[]))])), Vec::<String>::new());
    }

    #[test]
    fn the_command_is_one_up_that_waits() {
        assert_eq!(command("dc", Some(60)), "dc up -d --build --wait --wait-timeout 60");
        assert_eq!(command("dc", None), "dc up -d --build --wait");
    }

    #[test]
    fn a_rendered_file_waits_for_its_leaf_init_jobs_through_one_service() {
        // The store's init job is waited for by the app; the app's by nothing.
        let env = crate::testing::environment(
            "version: 1\nname: leaves\nnetworks: { net: { cidr: 10.71.0.0/24 } }\nmachines:\n  store: { networks: { net: 5 }, app: { image: redis }, setup: [fill.sh] }\n  app: { networks: { net: 6 }, depends_on: [store], app: { image: nginx }, setup: [prime.sh] }\n",
        );
        let rendered = crate::render::compose_file(crate::backend::ComposeTarget::Docker, &env).expect("renders");
        let done = rendered.file.services.get(JOBS_DONE).expect("the jobs' service");
        let waits: Vec<(&String, Condition)> = done.depends_on.iter().map(|(n, d)| (n, d.condition)).collect();
        assert_eq!(waits, [(&"isoloom-init-app-1".to_owned(), Condition::CompletedSuccessfully)]);
        assert!(!done.runs_once() && done.profiles.is_empty(), "started and waited for by `up --wait`");
        assert_eq!(leaf_jobs(&rendered.file.services), Vec::<String>::new(), "every job is waited for now");
        let plain = crate::testing::environment(
            "version: 1\nname: plain\nnetworks: { net: { cidr: 10.71.0.0/24 } }\nmachines:\n  web: { networks: { net: 5 }, app: { image: nginx } }\n",
        );
        let plain = crate::render::compose_file(crate::backend::ComposeTarget::Docker, &plain).expect("renders");
        assert!(!plain.file.services.contains_key(JOBS_DONE), "no job: no such service");
    }
}
