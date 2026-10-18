//! The checks: a Job per position, standing where that machine stands (its network labels, so
//! the same policies apply), else on every network. Each runner is a sh script next to the
//! Jobs, carried by a `ConfigMap` of their own; `exec` checks run inside the machine from a
//! runner of their own (`kubectl exec -i deploy/<machine> -- sh -s`), which the Jobs don't run.

use isoloom_plan::Environment;
use isoloom_plan::checks::{self, Host, Position, Probe, Resolved};
use isoloom_render::GeneratedFile;
use isoloom_render::container::{CHECK_IMAGE, exec_runner};

use crate::error::Result;
use crate::kustomize::{CHECK_SCRIPTS, Kustomization, key};
use crate::layout::output;
use crate::network::{self, CHECK_LABEL};
use crate::objects::{Container, Image, JobSpec, Manifest, Metadata, PodSpec, PodTemplate, Selector, Volume, VolumeMount};
use crate::pod::{config_map_volume, scripts_volume};
use crate::render::{document, documents, generated};

/// What the checks add to the environment.
#[derive(Default)]
pub(crate) struct Checks {
    /// One Job per position, as the plain manifests run them (and the chart's tests).
    pub(crate) jobs: Vec<Manifest>,
    /// The Jobs' runners, as (file name, contents).
    pub(crate) runners: Vec<(String, String)>,
    /// Every runner file, the `exec` ones included, in the order they are written.
    pub(crate) files: Vec<(String, String)>,
    /// The egress policies of the checks standing on offline networks.
    pub(crate) policies: Vec<Manifest>,
    /// The project's scripts the checks run.
    pub(crate) scripts: Vec<String>,
}

/// The environment's checks, grouped by position.
pub(crate) fn plan(env: &Environment) -> Checks {
    let mut out = Checks::default();
    let default = checks::default_position(env.spec());
    for (pos, group) in checks::by_position(env.spec(), &env.checks) {
        let job = if pos == default {
            "isoloom-check".to_owned()
        } else {
            format!("isoloom-check-{}", pos.id())
        };
        position(env, &pos, &job, group, &mut out);
    }
    out
}

/// Kubernetes picks pod addresses: machines are reached by name (their Service).
fn host(h: &Host, _: &Position) -> String {
    match h {
        Host::Literal(l) => l.clone(),
        Host::Machine { name, .. } => name.clone(),
    }
}

/// A project script, from the `ConfigMap` the pod mounts.
fn run_script(path: &str) -> String {
    format!("sh /isoloom/scripts/{}", key(path))
}

/// One position's runners, its Job and, standing on offline networks only, its egress policy
/// (offline as a machine there would be).
fn position(env: &Environment, pos: &Position, job: &str, group: Vec<&Resolved>, out: &mut Checks) {
    let render = checks::Render {
        host: &host,
        script: &run_script,
        playbook: None,
    };
    let nets: Vec<&String> = match pos {
        Position::Machine(m) => env.machines.get(m).map(|m| m.addresses.keys().collect()).unwrap_or_default(),
        Position::Networks => env.networks.keys().collect(),
    };
    out.scripts.extend(group.iter().filter_map(|c| match &c.probe {
        Probe::Script { path } => Some(path.clone()),
        _ => None,
    }));
    let (execs, group): (Vec<&Resolved>, Vec<&Resolved>) = group.into_iter().partition(|c| matches!(c.probe, Probe::Exec { .. }));
    if !execs.is_empty() {
        out.files.push((exec_runner(pos.id()), checks::script(pos, &execs, &render)));
    }
    let runner = (format!("{}.sh", pos.id()), checks::script(pos, &group, &render));
    out.files.push(runner.clone());
    if !nets.is_empty() && nets.iter().all(|n| env.networks.get(n.as_str()).is_some_and(|n| !n.internet)) {
        out.policies.push(network::offline(&format!("offline-{job}"), Selector::of(CHECK_LABEL, job)));
    }
    let mut labels = network::pod_labels(None, nets);
    labels.insert(CHECK_LABEL.to_owned(), job.to_owned());
    let check = Check {
        image: env.images.pull(CHECK_IMAGE),
        runner: &runner.0,
        scripts: group.iter().any(|c| matches!(c.probe, Probe::Script { .. })),
    };
    out.jobs.push(Manifest::job(job, check.spec(Metadata::labelled(labels))));
    out.runners.push(runner);
}

/// One position's check container.
struct Check<'a> {
    /// The check runner's image.
    image: String,
    /// Its runner's file name.
    runner: &'a str,
    /// Whether it runs project scripts.
    scripts: bool,
}

impl Check<'_> {
    /// Its Job: the runner, from the runners' `ConfigMap`, and the project's scripts when it runs
    /// some. Never retried: a failed check is the result.
    fn spec(&self, metadata: Metadata) -> JobSpec {
        let mut check = Container::new("check", Image::Tool(self.image.clone()));
        check.command = vec!["/bin/sh".into(), format!("/isoloom/run/{}", self.runner)];
        check.volume_mounts = vec![VolumeMount::at("runner", "/isoloom/run")];
        let mut volumes = vec![Volume {
            name: "runner".into(),
            source: config_map_volume(CHECK_SCRIPTS),
        }];
        if self.scripts {
            check.volume_mounts.push(VolumeMount::at("scripts", "/isoloom/scripts"));
            volumes.push(Volume {
                name: "scripts".into(),
                source: scripts_volume(),
            });
        }
        JobSpec {
            backoff_limit: 0,
            template: PodTemplate {
                metadata,
                spec: PodSpec {
                    restart_policy: Some("Never"),
                    containers: vec![check],
                    volumes,
                    ..PodSpec::default()
                },
            },
        }
    }
}

/// The plain checks' files in `.isoloom/kubernetes/checks/`: the runners, the Jobs (with how to
/// run them) and their kustomization. None without checks.
pub(crate) fn files(c: &Checks, namespace: &str) -> Result<Vec<GeneratedFile>> {
    if c.jobs.is_empty() {
        return Ok(Vec::new());
    }
    let file = |name: &str, contents: String| GeneratedFile {
        path: output(&format!("checks/{name}")),
        contents,
    };
    let mut out: Vec<GeneratedFile> = c.files.iter().map(|(n, text)| file(n, text.clone())).collect();
    let names: Vec<&str> = c.jobs.iter().map(|j| j.metadata.name.as_str()).collect();
    let usage = format!(
        "# The environment's checks, one Job per position. Run after the environment (or `isothrum check .`):\n#   kubectl -n {namespace} delete job {names} --ignore-not-found\n#   kubectl apply -k .isoloom/kubernetes/checks\n#   kubectl -n {namespace} wait --for=condition=complete job --all --timeout=300s; kubectl -n {namespace} logs -l {CHECK_LABEL}\n",
        names = names.join(" ")
    );
    out.push(file("job.yaml", format!("{}{usage}{}", generated(), documents(&c.jobs)?)));
    let runners = c.runners.iter().map(|(n, _)| n.clone()).collect();
    let k = Kustomization::new(namespace, "job.yaml", CHECK_SCRIPTS, runners);
    out.push(file("kustomization.yaml", format!("{}{}", generated(), document(&k)?)));
    Ok(out)
}
