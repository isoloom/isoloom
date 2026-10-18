//! How to start a Compose file: `wait` for everything but its leaf jobs (`up -d --wait`), then
//! run each of those jobs attached, in order, failing on its exit code. Computed from the typed
//! file this backend renders, and read back from a file's text the same way, so an embedder
//! holding only `.isoloom/docker/compose.yml` gets the plan Isoloom has.

use serde_yaml_ng::{Mapping, Value};

use crate::error::{Error, Result};
use crate::file::ComposeFile;

/// How to start a Compose file: the services to wait for, then the jobs to run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StartPlan {
    /// Services to bring up with `docker compose up -d --wait` (empty: all of them).
    pub wait: Vec<String>,
    /// One-shot jobs nothing running waits for, to run after `wait`, in order.
    pub jobs: Vec<String>,
}

/// A service as the start plan reads it: its name, what it waits for, whether it runs once
/// (`restart: "no"`). Services in a profile aren't started, so they aren't read.
struct Started {
    name: String,
    deps: Vec<String>,
    one_shot: bool,
}

/// The [`StartPlan`] of a Compose file's text: one-shots (`restart: "no"`) outside profiles that
/// no other started service depends on are jobs; when there are none, `wait` is empty (start
/// all).
///
/// # Errors
///
/// [`Error::Unreadable`] when the text isn't YAML. A file without services starts nothing
/// special: the default plan.
pub fn start_plan(compose_yaml: &str) -> Result<StartPlan> {
    let doc: Value = serde_yaml_ng::from_str(compose_yaml).map_err(|source| Error::Unreadable { source })?;
    let Some(Value::Mapping(services)) = doc.get("services") else {
        return Ok(StartPlan::default());
    };
    let started: Vec<Started> = services
        .iter()
        .filter_map(|(k, v)| Some((k.as_str()?, v.as_mapping()?)))
        .filter(|(_, v)| v.get("profiles").is_none())
        .map(|(name, v)| Started {
            name: name.to_owned(),
            deps: dependencies(v),
            one_shot: v.get("restart").and_then(Value::as_str) == Some("no"),
        })
        .collect();
    Ok(plan_of(&started))
}

/// What a service of a file's text waits for: `depends_on` as a map or as a list.
fn dependencies(service: &Mapping) -> Vec<String> {
    match service.get("depends_on") {
        Some(Value::Mapping(d)) => d.keys().filter_map(|k| k.as_str().map(String::from)).collect(),
        Some(Value::Sequence(d)) => d.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

impl StartPlan {
    /// The plan of a typed Compose file: what [`start_plan`] reads from its text.
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
    use proptest::prelude::*;

    use super::*;
    use crate::file::{Condition, Restart, Service};

    const FILE: &str = "
services:
  web: { restart: unless-stopped, depends_on: { db: { condition: service_healthy }, web-init-1: { condition: service_completed_successfully } } }
  web-init-1: { restart: 'no' }
  db: { restart: unless-stopped }
  seed: { restart: 'no', depends_on: [db] }
  isoloom-check: { profiles: [check], restart: 'no' }
";

    #[test]
    fn a_job_nothing_waits_for_runs_after_the_rest_is_up() {
        let plan = start_plan(FILE).expect("reads");
        assert_eq!(plan.jobs, ["seed"]);
        assert_eq!(plan.wait, ["web", "web-init-1", "db"]);
    }

    #[test]
    fn without_leaf_jobs_everything_is_waited_for() {
        assert_eq!(
            start_plan("services: { web: { restart: unless-stopped } }\n").expect("reads"),
            StartPlan::default()
        );
        assert_eq!(start_plan("name: empty\n").expect("reads"), StartPlan::default());
    }

    #[test]
    fn text_that_is_not_yaml_is_refused() {
        assert!(matches!(start_plan("services: [unclosed"), Err(Error::Unreadable { .. })));
    }

    /// A service of a generated file: whether it runs once, whether it is in a profile, and
    /// the services it waits for (by index; one out of range or its own is left out).
    fn service() -> impl Strategy<Value = (bool, bool, Vec<usize>)> {
        (any::<bool>(), any::<bool>(), prop::collection::vec(0usize..6, 0..3))
    }

    proptest! {
        #[test]
        fn the_text_and_the_typed_file_give_the_same_plan(services in prop::collection::vec(service(), 1..6)) {
            let mut file = ComposeFile::default();
            for (i, (once, profiled, deps)) in services.iter().enumerate() {
                let mut svc = Service {
                    restart: Some(if *once { Restart::No } else { Restart::UnlessStopped }),
                    ..Service::default()
                };
                if *profiled {
                    svc.profiles = vec!["check".into()];
                }
                for d in deps.iter().filter(|d| **d < services.len() && **d != i) {
                    svc.after(format!("s{d}"), Condition::Started);
                }
                file.services.insert(format!("s{i}"), svc);
            }
            let yaml = serde_yaml_ng::to_string(&file).expect("writes");
            prop_assert_eq!(StartPlan::of(&file), start_plan(&yaml).expect("reads back"));
        }
    }
}
