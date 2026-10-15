//! Isoloom's Kubernetes backend: the environment's containers on a Kubernetes cluster, in
//! `.isoloom/kubernetes/` (plain manifests, applied through kustomize so the project's scripts
//! travel as a `ConfigMap`), and a Helm chart of the same objects, written from typed objects.
//!
//! - A namespace per environment; each machine a Deployment (one replica) with its hostname,
//!   and a Service named after it when it serves, so names resolve as on every other target.
//! - Networks and `reach`: `NetworkPolicy` objects (deny by default; a network's machines reach
//!   each other; `reach` rules open one network to another, on their ports). No router: the
//!   cluster's network policy engine enforces them (k3s, Calico, Cilium; not every CNI does).
//! - Offline machines: an egress policy keeps them inside the environment (and DNS).
//! - `publish`: a `LoadBalancer` Service on the published port (local clusters such as OrbStack,
//!   Docker Desktop and k3s answer on localhost).
//! - Volumes: a `PersistentVolumeClaim` each, seeded from the image as Docker seeds a named
//!   volume. `init:` jobs: a container in the machine's pod (same network and volumes), run once
//!   the machine answers. `depends_on`: init containers that wait for the dependencies'
//!   services.
//! - Checks: a Job per position in `.isoloom/kubernetes/checks/`, with the networks (labels) of
//!   the machine it stands for, so the same policies apply to it.
//! - Addresses: Kubernetes picks pod addresses; names, ports and reachability are kept.
//! - Images: as the plan resolves them (through the registry mirrors), Isoloom's own included.
//! - The Helm chart in `.isoloom/kubernetes/chart/`: the machines' images and the inputs as
//!   values (typed by `values.schema.json`, so a mistyped one is rejected), the checks as
//!   `helm test` Pods, the project's scripts under `files/project/` (copied there by the command
//!   line, see [`project_copies`]).
//!
//! What Kubernetes can't do is refused from the plan's requirements: environment-level
//! provisioning, Ansible checks, `exec` checks without a container, network appliances,
//! aliases, gateway machines, ulimits and `tc`.
//!
//! It sits with the other backends, above the rendering layer (`isoloom-render`) and below the
//! targets' registry (`isoloom-targets`), and depends on no other backend: [`KubernetesBackend`]
//! renders a planned [`Environment`](isoloom_plan::Environment), purely, into files;
//! [`project_copies`] is the one function that reads the project folder.
//!
//! # Example
//!
//! ```
//! use isoloom_kubernetes::KubernetesBackend;
//! use isoloom_plan::{Environment, Lab};
//! use isoloom_render::backend::Backend;
//! use isoloom_spec::model::Target;
//!
//! let lab = Lab::new(isoloom_spec::parse(
//!     "version: 1
//! name: hello
//! networks: { lan: { cidr: 10.9.0.0/24 } }
//! machines:
//!   web: { networks: { lan: 10 }, ports: { web: 80 }, app: { image: nginx } }
//! ",
//! )?);
//! let env = Environment::build(&lab.for_target(Target::Kubernetes), Target::Kubernetes, None);
//! let files = KubernetesBackend.render(&env)?;
//! let manifests = files.iter().find(|f| f.path == ".isoloom/kubernetes/environment.yaml").ok_or("no manifests")?;
//! assert!(manifests.contents.contains("kind: Deployment"));
//! assert!(files.iter().any(|f| f.path == ".isoloom/kubernetes/chart/Chart.yaml"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod backend;
mod chart;
mod checks;
mod dns;
mod error;
mod files;
mod gpus;
mod ipv6;
mod kustomize;
mod layout;
mod machine;
mod network;
mod objects;
mod pod;
mod process;
mod project;
mod render;
mod secrets;
mod storage;
#[cfg(test)]
mod testing;
mod values_schema;

pub use backend::KubernetesBackend;
pub use error::{Error, Result};
pub use network::network_member;
pub use project::project_copies;
pub use secrets::SECRETS;
