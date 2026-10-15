//! The Kubernetes files of a planned [`Environment`]: the namespace, each container machine's
//! objects, the networks' policies and the checks, written as plain manifests with their
//! kustomizations, then as the Helm chart.

use std::fmt;

use isoloom_plan::{Environment, Refusal, Requirement};
use isoloom_render::backend::Files;
use isoloom_render::{GeneratedFile, header};
use serde::Serialize;

use crate::backend::{capabilities, reason};
use crate::checks::{self, Checks};
use crate::error::{Error, Result};
use crate::layout::output;
use crate::machine::{self, Machine};
use crate::objects::Manifest;
use crate::pod::INPUTS;
use crate::{chart, kustomize, network};

/// The environment's objects and what goes with them.
pub(crate) struct Rendered {
    /// Its namespace.
    pub(crate) namespace: String,
    /// Every object, the namespace first.
    pub(crate) objects: Vec<Manifest>,
    /// The checks.
    pub(crate) checks: Checks,
    /// The project's scripts the pods run (init jobs, checks), sorted, each once.
    pub(crate) scripts: Vec<String>,
}

/// The files of `env` on Kubernetes: the manifests and their kustomization, the checks', then
/// the chart. Refused, without rendering, when `env` needs what Kubernetes can't do.
pub(crate) fn render(env: &Environment) -> Result<Files> {
    if let Some(refusal) = isoloom_plan::refusal(&env.requirements, &capabilities()) {
        return Err(refused(refusal));
    }
    if let Some(reason) = crate::gpus::refusal(env) {
        return Err(Error::Unsupported { reason });
    }
    let r = objects(env);
    let mut files = vec![
        GeneratedFile {
            path: output("kustomization.yaml"),
            contents: kustomization(&r)?,
        },
        GeneratedFile {
            path: output("environment.yaml"),
            contents: manifests(env, &r)?,
        },
    ];
    files.extend(checks::files(&r.checks, &r.namespace)?);
    files.extend(chart::files(env, &r)?);
    files.extend(isoloom_render::dockerfile::files(env));
    Ok(files)
}

/// The refusal as the backend reports it: the plan's, but in Kubernetes' own words where they
/// differ from the plan's (see [`reason`]).
fn refused(refusal: Refusal) -> Error {
    match &refusal.requirement {
        Requirement::Gateway { .. } => Error::Unsupported { reason: reason(&refusal) },
        _ => Error::Refused { refusal },
    }
}

/// The environment's objects, in the order they are written.
fn objects(env: &Environment) -> Rendered {
    let namespace = format!("isoloom-{}", env.name);
    let machines = Machine::all(env);
    let mut ns = Manifest::namespace(&namespace);
    ns.metadata.labels = crate::storage::namespace_labels(&machines);
    let mut objects = vec![ns];
    objects.extend(machines.iter().flat_map(machine::objects));
    objects.extend(network::policies(env));
    let checks = checks::plan(env);
    objects.extend(checks.policies.iter().cloned());
    let mut scripts: Vec<String> = machines.iter().flat_map(|m| m.init().iter().cloned()).collect();
    scripts.extend(machines.iter().flat_map(crate::files::sources));
    scripts.extend(checks.scripts.iter().cloned());
    scripts.sort();
    scripts.dedup();
    Rendered {
        namespace,
        objects,
        checks,
        scripts,
    }
}

/// The plain manifests, with how to start and stop them, the images to build first and the
/// inputs' Secret to create.
fn manifests(env: &Environment, r: &Rendered) -> Result<String> {
    let usage = Usage { env, namespace: &r.namespace };
    Ok(format!("{}{usage}\n{}", generated(), documents(&r.objects)?))
}

/// How to run the plain manifests, at their top: start and stop, the images built from the
/// project, the inputs' Secret, the secrets' Secret.
struct Usage<'a> {
    env: &'a Environment,
    namespace: &'a str,
}

impl fmt::Display for Usage<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (env, ns) = (self.env, self.namespace);
        writeln!(
            f,
            "# Start:  kubectl apply -k .isoloom/kubernetes\n#         kubectl -n {ns} wait --for=condition=Available deployment --all --timeout=600s\n# Stop:   kubectl delete namespace {ns}"
        )?;
        let builds: Vec<String> = Machine::all(env).iter().filter_map(Machine::build_command).collect();
        if !builds.is_empty() {
            f.write_str("# Images built from the project, before starting (a local cluster sharing Docker's images:\n# OrbStack, Docker Desktop; elsewhere, push them to a registry the cluster pulls from):\n")?;
            for b in &builds {
                writeln!(f, "#   {b}")?;
            }
        }
        if env.machines.values().any(|m| !m.inputs.is_empty()) {
            let literals: Vec<String> = env.spec().inputs.iter().map(|n| format!("--from-literal={n}=…")).collect();
            writeln!(
                f,
                "# Inputs (optional, unset ones are empty): kubectl -n {ns} create secret generic {INPUTS} {}",
                literals.join(" ")
            )?;
        }
        if let Some(usage) = crate::secrets::usage(env, ns) {
            f.write_str(&usage)?;
        }
        Ok(())
    }
}

/// The environment's kustomization: its manifests, and the project's scripts as a `ConfigMap`.
fn kustomization(r: &Rendered) -> Result<String> {
    let mut out = generated();
    if !r.scripts.is_empty() {
        out.push_str("# The project's scripts the pods run (init jobs, checks), from the chart's copies (`isoloom generate` writes them).\n");
    }
    out.push_str(&document(&kustomize::environment(&r.namespace, &r.scripts))?);
    Ok(out)
}

/// The first lines of every generated YAML file.
pub(crate) fn generated() -> String {
    header("#")
}

/// One YAML document.
pub(crate) fn document<T: Serialize>(value: &T) -> Result<String> {
    serde_yaml_ng::to_string(value).map_err(|source| Error::Unwritable { source })
}

/// Several YAML documents, one file.
pub(crate) fn documents(objects: &[Manifest]) -> Result<String> {
    let docs: Vec<String> = objects.iter().map(document).collect::<Result<_>>()?;
    Ok(docs.join("---\n"))
}
