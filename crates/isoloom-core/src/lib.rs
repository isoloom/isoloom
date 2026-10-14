//! Isoloom: describe an environment once (machines, networks, the services they expose),
//! run it anywhere. Each target produces the same external behavior its own way: containers,
//! VMs on this machine, a Proxmox server, or the cloud.
//!
//! This crate is the format itself: parse, validate, derive the targets a spec can run on,
//! and add up what it needs; generate each target's files (Docker Compose, hosted, Kubernetes,
//! hybrid, Vagrant, Docker on one VM, Docker on one cloud VM, Proxmox, one cloud VM per
//! machine, existing hosts, cloud services: see [`model::Target`]); and track what each output
//! does with every field ([`coverage`]).
//!
//! # Stability
//!
//! The crate is embedded by other tools (as a git dependency, pinned to a tag), so part of it is
//! a supported library surface and the rest is Isoloom's own:
//!
//! - **Supported**: the items at the crate root ([`load`], [`load_with`], [`parse`], [`find`],
//!   [`validate()`], [`validate_files`], [`derive`](fn@derive), [`effective`], [`generate()`],
//!   [`generate_all`], [`generate_instance`], [`refusal`], [`refusal_on`], [`totals`], the
//!   re-exported model types) and the modules [`model`], [`validate`](mod@validate),
//!   [`targets`], [`generate`](mod@generate) (with [`resolved`], the snapshot), [`checks`] (the
//!   check plan and the runner's output lines), [`instance`], [`defaults`], [`profile`], [`registry`]
//!   (`~/.isoloom/status.yml`, see `docs/registry.md`), [`host`], and [`events`] (the lifecycle's
//!   JSON events and errors, with an emitter to receive them; see `docs/json.md`).
//! - **Internal**: every other module ([`clones`], [`groups`], [`vlans`], [`ignore`], [`images`],
//!   [`import`], [`coverage`], [`report`], [`schema`], [`shell`]) is public for the `isoloom`
//!   command line and may change in any release. Helpers no other crate needs are
//!   `pub(crate)`; more will follow.
//!
//! While Isoloom is at 0.x, a breaking change to the supported surface (an item removed or
//! renamed, a signature or a public field changed) bumps the minor version (0.10 -> 0.11), never
//! the patch, and gets a "Breaking (library)" line in the CHANGELOG. A new public field or enum
//! variant isn't breaking (a new spec key is one): get the model's types by parsing a spec, build
//! the others with `..Default::default()` where they have one, and match enums with a `_` arm
//! where a new variant could appear. Release tags (`v0.10.1`)
//! are the pinning points: depend on a tag, not on a branch. The files other tools read have
//! their own contracts: `~/.isoloom/status.yml` carries a `version:` ([`registry::VERSION`]),
//! the resolved snapshot a `version` ([`resolved::RESOLVED_VERSION`]), and the cloud modules'
//! variables and outputs are listed in `docs/cloud-modules.md`.

pub use isoloom_coverage as coverage;
pub use isoloom_import as import;
pub use isoloom_plan::checks;
pub use isoloom_render::ignore;
pub use isoloom_runtime::{RegistryError, events, host, registry, report, tool};
pub use isoloom_spec::{clones, defaults, groups, images, instance, model, profile, schema, shell, targets, vlans};
pub use isoloom_targets as generate;
pub use isoloom_targets::resolved;

#[allow(deprecated)]
pub use generate::qemu_refusal;
pub use generate::{GenerateError, GeneratedFile, OUTPUT_DIR, generate, generate_all, generate_instance, project_copies, refusal, refusal_on};
pub use isoloom_spec::validate::{self, Problem, validate, validate_files};
pub use isoloom_spec::{
    DEFAULT_CPUS, DEFAULT_DISK_GB, DEFAULT_MEMORY_MB, LoadError, SPEC_FILES, Stage, Totals, find, load, load_profiled, load_with, parse, totals,
};
pub use model::{
    Appliance, Arch, Check, CloudServices, Declared, Dns, Expect, KNOWN_OS, Machine, Network, Reach, Resources, Service, Shape, Spec, Target, Tc, Vlan,
};
pub use targets::{derive, effective};
