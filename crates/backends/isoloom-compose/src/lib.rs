//! Isoloom's Docker Compose backend: the Compose file the `docker` and `hosted` targets run (and
//! the Docker-on-a-VM targets carry), at `.isoloom/docker/compose.yml`, with the files it mounts
//! and the `docker` target's run manifest.
//!
//! - Each network becomes a Compose network with its subnet (Docker at `.1`, or at the last
//!   address when a machine is the network's gateway); `internet: false` makes it `internal`
//!   when nothing routes between networks.
//! - A machine that is a network's gateway gets forwarding and `NET_ADMIN`; the machines
//!   behind it route everything through it (in a sidecar, once it answers).
//! - Each machine becomes a service with its fixed address on every network; services find
//!   each other by name (Compose DNS).
//! - Machines with services get a healthcheck (a TCP probe, and the machine's own `healthcheck`
//!   beside it), so `depends_on` waits for them to answer and `docker compose up --wait` means
//!   "everything answers". The probe is Isoloom's own static busybox (a one-shot
//!   `isoloom-probe-<arch>` copies it into a volume), so it needs nothing from the image:
//!   distroless and `scratch` machines work too.
//! - `init:` scripts run once, in one-shot containers of the machine's image on its networks,
//!   after it answers; machines depending on it wait for them to finish.
//! - Inputs become environment variables read from the shell (`${NAME:-}`), only on the
//!   machines that list them.
//! - Checks run in a `check` profile, one runner per position (`isoloom-check` where a user
//!   stands, `isoloom-check-<machine>` for the others), each a sh script in `checks/`.
//! - A service's `publish` port is published on the host's loopback (127.0.0.1) only, on an
//!   ephemeral host port Docker picks, so two labs publishing the same port never collide; the
//!   runner reads the real port back from `docker compose ps`. `ISOLOOM_PUBLISH_ADDRESS`
//!   overrides the address and `ISOLOOM_PUBLISH_FIXED` pins the host port to `publish` (both
//!   set inside the docker-vm and cloud VMs, where the lab is alone and that fixed port is
//!   forwarded from loopback).
//! - `volumes:` become named volumes: they survive re-creating a container, and go with
//!   `docker compose down -v`.
//! - Network appliances (`docker.appliance`) run router and switch OSes in containers, wired
//!   the way their image expects.
//!
//! It sits with the other backends, above the rendering layer (`isoloom-render`) and below the
//! targets' registry (`isoloom-targets`), and depends on no other backend: [`ComposeBackend`]
//! renders a planned [`Environment`](isoloom_plan::Environment), purely, into files, through
//! typed Compose structs. [`hybrid`] adapts the same file to the `hybrid` target's Docker host,
//! and [`start_plan`] reads back how to start a file an embedder holds.
//!
//! # Example
//!
//! ```
//! use isoloom_compose::{ComposeBackend, start_plan};
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
//! let env = Environment::build(&lab.for_target(Target::Docker), Target::Docker, None);
//! let files = ComposeBackend(Target::Docker).render(&env)?;
//! let compose = files.iter().find(|f| f.path == ".isoloom/docker/compose.yml").ok_or("no Compose file")?;
//! assert!(compose.contents.contains("ipv4_address: 10.9.0.10"));
//! // Nothing runs once and exits: everything is waited for.
//! assert_eq!(start_plan(&compose.contents)?.jobs, Vec::<String>::new());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod appliances;
mod backend;
mod check_runners;
mod error;
mod file;
pub mod hybrid;
mod images;
mod interpolation;
mod ipv6;
mod labels;
mod layout;
mod machine;
mod manifest;
mod names;
mod networks;
mod outside;
mod render;
mod router;
mod sidecars;
mod start;
mod storage;
#[cfg(test)]
mod testing;

pub use backend::{ComposeBackend, capabilities_on_a_vm, guest};
pub use error::{Error, Result};
pub use labels::{ENVIRONMENT_LABEL, MANAGED_LABEL};
pub use render::render;
pub use start::{StartPlan, start_plan};
