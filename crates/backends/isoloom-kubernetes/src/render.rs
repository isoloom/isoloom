//! The Kubernetes files of a planned [`Environment`]: the namespace, each container machine's
//! objects, the networks' policies and the checks, written as plain manifests with their
//! kustomizations, then as the Helm chart.

use std::fmt::Write;

use isoloom_plan::{Environment, Refusal, Requirement};
use isoloom_render::backend::{Files, RenderError};
use isoloom_render::{GenerateError, GeneratedFile, header};
use isoloom_spec::model::Target;
use serde::Serialize;

use crate::checks::{self, Checks};
use crate::machine::{self, Machine};
use crate::objects::Manifest;
use crate::pod::INPUTS;
use crate::{capabilities, chart, kustomize, network, output};

/// The environment's objects and what goes with them.
pub(crate) struct Rendered {
    /// Its namespace.
    pub namespace: String,
    /// Every object, the namespace first.
    pub objects: Vec<Manifest>,
    /// The checks.
    pub checks: Checks,
    /// The project's scripts the pods run (init jobs, checks), sorted, each once.
    pub scripts: Vec<String>,
}

/// The files of `env` on Kubernetes: the manifests and their kustomization, the checks', then
/// the chart. Refused, without rendering, when `env` needs what Kubernetes can't do.
pub fn render(env: &Environment) -> Result<Files, RenderError> {
    if let Some(refusal) = isoloom_plan::refusal(&env.requirements, &capabilities()) {
        return Err(refused(refusal));
    }
    if let Some(what) = crate::gpus::refusal(env) {
        return Err(RenderError::Generate(GenerateError::Unsupported {
            target: Target::Kubernetes,
            what,
        }));
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
fn refused(refusal: Refusal) -> RenderError {
    match &refusal.requirement {
        Requirement::Gateway { .. } => RenderError::Generate(GenerateError::Unsupported {
            target: Target::Kubernetes,
            what: reason(&refusal),
        }),
        _ => RenderError::Refused {
            target: Target::Kubernetes,
            refusal,
        },
    }
}

/// Why Kubernetes refuses: the plan's reason, but for gateway machines, which the plan words for
/// the hybrid target.
pub fn reason(refusal: &Refusal) -> String {
    match refusal.requirement {
        Requirement::Gateway { .. } => "networks with a `gateway` machine on Kubernetes come later".into(),
        _ => refusal.to_string(),
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
    let mut scripts: Vec<String> = machines.iter().flat_map(|m| m.init().iter().chain(&m.plan.process.before).cloned()).collect();
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
fn manifests(env: &Environment, r: &Rendered) -> Result<String, RenderError> {
    let ns = &r.namespace;
    let mut out = generated();
    let _ = writeln!(
        out,
        "# Start:  kubectl kustomize --load-restrictor LoadRestrictionsNone .isoloom/kubernetes | kubectl apply -f -\n#         kubectl -n {ns} wait --for=condition=Available deployment --all --timeout=600s\n# Stop:   kubectl delete namespace {ns}"
    );
    let builds: Vec<String> = Machine::all(env).iter().filter_map(Machine::build_command).collect();
    if !builds.is_empty() {
        out.push_str("# Images built from the project, before starting (a local cluster sharing Docker's images:\n# OrbStack, Docker Desktop; elsewhere, push them to a registry the cluster pulls from):\n");
        for b in &builds {
            let _ = writeln!(out, "#   {b}");
        }
    }
    if env.machines.values().any(|m| !m.inputs.is_empty()) {
        let literals: Vec<String> = env.spec().inputs.iter().map(|n| format!("--from-literal={n}=…")).collect();
        let _ = writeln!(
            out,
            "# Inputs (optional, unset ones are empty): kubectl -n {ns} create secret generic {INPUTS} {}",
            literals.join(" ")
        );
    }
    if let Some(usage) = crate::secrets::usage(env, ns) {
        out.push_str(&usage);
    }
    out.push('\n');
    out.push_str(&documents(&r.objects)?);
    Ok(out)
}

/// The environment's kustomization: its manifests, and the project's scripts as a ConfigMap.
fn kustomization(r: &Rendered) -> Result<String, RenderError> {
    let mut out = generated();
    if !r.scripts.is_empty() {
        out.push_str("# The project's scripts the pods run (init jobs, checks), read from the project at apply time.\n");
    }
    out.push_str(&document(&kustomize::environment(&r.namespace, &r.scripts))?);
    Ok(out)
}

/// The first lines of every generated YAML file.
pub(crate) fn generated() -> String {
    header("#")
}

/// One YAML document.
pub(crate) fn document<T: Serialize>(value: &T) -> Result<String, RenderError> {
    serde_yaml_ng::to_string(value).map_err(|e| unwritable(&e))
}

/// Several YAML documents, one file.
pub(crate) fn documents(objects: &[Manifest]) -> Result<String, RenderError> {
    let docs: Vec<String> = objects.iter().map(document).collect::<Result<_, _>>()?;
    Ok(docs.join("---\n"))
}

/// A YAML error (an object that doesn't serialize), as the backend reports it.
pub(crate) fn unwritable(e: &serde_yaml_ng::Error) -> RenderError {
    RenderError::Generate(GenerateError::Unsupported {
        target: Target::Kubernetes,
        what: format!("a manifest doesn't serialize: {e}"),
    })
}
