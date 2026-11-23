//! What the Kubernetes backend's integration tests share: examples and specs planned and
//! rendered, their files read.

use std::error::Error;
use std::path::Path;

use isoloom_kubernetes::KubernetesBackend;
use isoloom_plan::{Environment, Lab};
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

/// A test's outcome: a failed step is its report.
pub(crate) type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The environment of `spec` (the authored format), which must be valid, planned for Kubernetes.
pub(crate) fn planned(spec: isoloom_spec::Spec) -> Outcome<Environment> {
    let lab = Lab::new(spec);
    let problems = lab.problems();
    if !problems.is_empty() {
        return Err(format!("the spec isn't valid: {problems:?}").into());
    }
    Ok(lab.plan(&lab.for_target(Target::Kubernetes), Target::Kubernetes, None)?)
}

/// An example of the project, as written.
pub(crate) fn load(example: &str) -> Outcome<isoloom_spec::Spec> {
    Ok(isoloom_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(example),
    )?)
}

/// An example of the project, planned for Kubernetes.
pub(crate) fn plan(example: &str) -> Outcome<Environment> {
    planned(load(example)?)
}

/// A spec written in a test, planned for Kubernetes.
pub(crate) fn parsed(yaml: &str) -> Outcome<Environment> {
    planned(isoloom_spec::parse(yaml)?)
}

/// The files the backend renders for an example, from its plan.
pub(crate) fn render(example: &str) -> Outcome<Vec<GeneratedFile>> {
    Ok(KubernetesBackend.render(&plan(example)?)?)
}

/// The files whose path passes `keep`, one after the other under their path.
pub(crate) fn joined(files: &[GeneratedFile], keep: impl Fn(&str) -> bool) -> String {
    let parts: Vec<String> = files
        .iter()
        .filter(|f| keep(&f.path))
        .map(|f| format!("=== {}\n{}\n", f.path, f.contents))
        .collect();
    parts.concat()
}

pub(crate) fn manifests(example: &str) -> Outcome<String> {
    Ok(joined(&render(example)?, |p| !p.contains("/chart/")))
}

pub(crate) fn chart(example: &str) -> Outcome<String> {
    Ok(joined(&render(example)?, |p| p.contains("/chart/")))
}

/// The text of the one file among `files` whose path ends with `suffix`.
pub(crate) fn file<'a>(files: &'a [GeneratedFile], suffix: &str) -> Outcome<&'a str> {
    let found = files.iter().find(|f| f.path.ends_with(suffix)).ok_or_else(|| format!("no {suffix}"))?;
    Ok(&found.contents)
}

pub(crate) const MIRRORED: &str = "version: 1
name: shop
networks: { app: { cidr: 10.60.0.0/24 } }
machines:
  web:
    networks: { app: 10 }
    ports: { web: 80 }
    environment: { GREETING: 'hi {{ there }}' }
    app: { image: nginx }
  db:
    networks: { app: 20 }
    ports: { db: 5432 }
    depends_on: [web]
    app: { build: db }
constraints:
  registries: { docker.io: registry.corp/hub }
";

/// A spec with one machine, `web`, on one network, `lan`, and `extra` lines for the machine.
pub(crate) const BASE: &str =
    "version: 1\nname: r\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  web:\n    networks: { lan: 10 }\n    app: { image: nginx }\n";
