//! The Kubernetes backend: the environment's containers on a Kubernetes cluster, in
//! `.isoloom/kubernetes/` (plain manifests, applied through kustomize so the project's scripts
//! travel as a ConfigMap), and a Helm chart of the same objects. It renders a planned
//! [`Environment`] through typed objects ([`objects`]).
//!
//! - A namespace per environment; each machine a Deployment (one replica) with its hostname,
//!   and a Service named after it when it serves, so names resolve as on every other target.
//! - Networks and `reach`: NetworkPolicies (deny by default; a network's machines reach each
//!   other; `reach` rules open one network to another, on their ports). No router: the cluster's
//!   network policy engine enforces them (k3s, Calico, Cilium; not every CNI does).
//! - Offline machines: an egress policy keeps them inside the environment (and DNS).
//! - `publish`: a LoadBalancer Service on the published port (local clusters such as OrbStack,
//!   Docker Desktop and k3s answer on localhost).
//! - Volumes: a PersistentVolumeClaim each, seeded from the image as Docker seeds a named
//!   volume (see [`pod`]). `init:` jobs: a container in the machine's pod (same network and
//!   volumes), run once the machine answers. `depends_on`: init containers that wait for the
//!   dependencies' services.
//! - Checks: a Job per position in `.isoloom/kubernetes/checks/`, with the networks (labels) of
//!   the machine it stands for, so the same policies apply to it.
//! - Addresses: Kubernetes picks pod addresses; names, ports and reachability are kept.
//! - Images: as the plan resolves them (through the registry mirrors), Isoloom's own included.
//! - The Helm chart in `.isoloom/kubernetes/chart/` ([`chart`]): the machines' images and the
//!   inputs as values (typed by `values.schema.json`, so a mistyped one is rejected), the checks as `helm test` Pods, the project's scripts under
//!   `files/project/` (copied there by the command line, see [`project_copies`]).
//!
//! What Kubernetes can't do is refused from the plan's requirements: environment-level
//! provisioning, Ansible checks, `exec` checks without a container, network appliances,
//! aliases, gateway machines, ulimits and `tc` ([`capabilities`]).

// The lints the workspace still tracks (#106) hold here already.
#![deny(
    missing_docs,
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::expect_used,
    clippy::format_push_string,
    clippy::format_collect,
    clippy::map_unwrap_or,
    clippy::cast_possible_truncation,
    clippy::match_wildcard_for_single_variants
)]

pub mod chart;
mod checks;
mod dns;
mod ipv6;
pub mod kustomize;
mod machine;
mod network;
pub mod objects;
pub mod pod;
mod render;
pub mod secrets;
mod values_schema;

use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_render::{GeneratedFile, OUTPUT_DIR};
use isoloom_spec::model::Target;

pub use render::{reason, render};

/// The target's folder, under `.isoloom/`.
pub const DIR: &str = "kubernetes";
/// Where the chart is, under the target's folder.
pub const CHART: &str = "chart";
/// Where the chart's copies of the project's scripts go, under the chart.
pub const CHART_PROJECT_FILES: &str = "files/project";

/// What Kubernetes can render: IPv6 networks of every kind (the cluster picks pod addresses,
/// so a network's IPv6 side sets the Services' IP families) and secrets (a Secret `isoloom run`
/// creates). No controller (no `provision:`, no Ansible checks), `exec` checks run inside a
/// container, no network appliances, Service names have no dots, no gateway machine routes a
/// network, containers have no ulimits, with no router in the path there is nothing to impair
/// links with, and a Secret volume's files belong to root.
pub fn capabilities() -> Capabilities {
    Capabilities::of([Feature::Ipv6, Feature::Ipv6Only, Feature::Ipv6Auto, Feature::Secrets])
}

/// The `kubernetes` target's backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct KubernetesBackend;

impl Backend for KubernetesBackend {
    fn target(&self) -> Target {
        Target::Kubernetes
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        render(env)
    }

    /// The plan's refusal, in Kubernetes' words ([`reason`]).
    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &capabilities()).map(|r| reason(&r))
    }
}

/// Why the chart's copies of the project's scripts couldn't be made.
#[derive(Debug, thiserror::Error)]
pub enum ProjectCopyError {
    /// A script the pods run exists in the project but can't be read.
    #[error("{path} (copied into the Helm chart): {source}")]
    Read {
        /// The script, from the project folder.
        path: String,
        /// Why it couldn't be read.
        #[source]
        source: std::io::Error,
    },
    /// A generated kustomization isn't one (it was edited by hand).
    #[error("{path}: not a kustomization Isoloom reads: {source}")]
    Kustomization {
        /// The kustomization, from the project folder.
        path: String,
        /// Why it couldn't be read.
        #[source]
        source: serde_yaml_ng::Error,
    },
}

/// The project's own files a target carries with it, read from the project folder `dir`: the
/// Helm chart's copies of the scripts its pods run (`chart/files/project/`), so the chart works
/// away from the project. Generators only see the spec, so whoever writes `files` adds these:
/// `isoloom generate`, `run` and `check` do.
pub fn project_copies(files: &[GeneratedFile], dir: &std::path::Path) -> Result<Vec<GeneratedFile>, ProjectCopyError> {
    let mut out = Vec::new();
    for f in files.iter().filter(|f| f.path.ends_with(&format!("/{DIR}/kustomization.yaml"))) {
        let base = f.path.trim_end_matches("kustomization.yaml");
        let scripts = kustomize::project_scripts(&f.contents).map_err(|source| ProjectCopyError::Kustomization { path: f.path.clone(), source })?;
        for (key, path) in scripts {
            // A file the project doesn't have yet is `validate`'s to report (and the chart's
            // `required` names it at install): generating goes on without it.
            let contents = match std::fs::read_to_string(dir.join(&path)) {
                Ok(c) => c,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(source) => return Err(ProjectCopyError::Read { path, source }),
            };
            out.push(GeneratedFile {
                path: format!("{base}{CHART}/{CHART_PROJECT_FILES}/{key}"),
                contents,
            });
        }
    }
    Ok(out)
}

/// Where the target's files go, from the project folder.
fn output(path: &str) -> String {
    format!("{OUTPUT_DIR}/{DIR}/{path}")
}
