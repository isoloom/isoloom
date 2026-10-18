//! The Ansible configuration-management renderer (design `plan/phase4/design.md` §2, §6.1): a
//! machine's resolved recipe ([`Recipe`](isoloom_plan::configure::Recipe)) as native tasks from
//! `ansible.builtin` and `ansible.posix` only, in a generated role per machine
//! (`isoloom_machine_<m>`), run by `site.yml` through the one Ansible inventory (the controller on
//! VM targets, the user on the `external` target). It reaches the same end state as the shell
//! library, so the forms stay equal; the resource conformance harness proves it.
//!
//! - [`project`]: an environment's whole Ansible project (`site.yml`, the roles, and
//!   `requirements.yml` when a role names a pinned collection), under a target's folder.
//! - [`role_files`] and [`role_name`]: one machine's role, which the conformance harness renders
//!   and runs on its own.
//!
//! Pure: it reads the planned environment and the embedded catalogue's pinned collections, and
//! writes files. No vendor or `community.*` module is ever named (the maintainer's rule).

mod role;
mod services;
mod task;
mod tasks;

use isoloom_catalogue::Catalogue;
use isoloom_plan::Environment;
use isoloom_plan::configure::Resource;
use isoloom_render::configure::Configured;
use isoloom_render::inventory::hosts;
use isoloom_render::{GeneratedFile, header};
use serde_json::{Value, json};

pub use role::{files as role_files, role_name};

/// The project variable each play sets to the project folder, so a role's `copy` and `script`
/// find a project file wherever `site.yml` is run from (relative to the playbook's own folder).
const PROJECT_VAR: &str = "{{ (playbook_dir + '/../..') | realpath }}";

/// The Ansible project for `env`, its files under `base` (`.isoloom/external`): `site.yml` running
/// each host's role, the roles themselves, and `requirements.yml` when a role names a pinned
/// collection. A host without a recipe (a bare user machine) gets no role and no play.
#[must_use]
pub fn project(env: &Environment, base: &str) -> Vec<GeneratedFile> {
    let configured: Vec<(String, Configured<'_>)> = hosts(env)
        .into_iter()
        .filter_map(|h| Some((h.name.to_owned(), Configured::of(env, h.name)?)))
        .collect();
    if configured.is_empty() {
        return Vec::new();
    }
    let mut out = vec![site(base, &configured)];
    for (name, c) in &configured {
        out.extend(role_files(c, &format!("{base}/roles/{}", role_name(name))));
    }
    if let Some(reqs) = requirements(&configured) {
        out.push(GeneratedFile {
            path: format!("{base}/requirements.yml"),
            contents: reqs,
        });
    }
    out
}

/// `site.yml`: one play per configured host, applying its role as `isoloom` the host over the
/// inventory, each play setting [`PROJECT_VAR`].
fn site(base: &str, configured: &[(String, Configured<'_>)]) -> GeneratedFile {
    let plays: Vec<Value> = configured
        .iter()
        .map(|(name, _)| {
            json!({
                "name": format!("Configure {name}"),
                "hosts": name,
                "become": true,
                "vars": { "isoloom_project": PROJECT_VAR },
                "roles": [role_name(name)],
            })
        })
        .collect();
    GeneratedFile {
        path: format!("{base}/site.yml"),
        contents: format!("{}{}", header("#"), task::yaml(plays)),
    }
}

/// `requirements.yml` naming the pinned collections the roles use (today only `ansible.posix`,
/// for a `firewall` step), from the embedded catalogue; `None` when no role names one.
///
/// Each entry carries the exact version, its archive `source`, and the `sha256` that
/// `catalogue/collections.yml` pins (never dropped): a run checks the archive against it before
/// installing, so it can't fetch a mutated collection (the design's vendored/pinned rule).
fn requirements(configured: &[(String, Configured<'_>)]) -> Option<String> {
    let firewall = configured
        .iter()
        .any(|(_, c)| c.recipe().resources().any(|r| matches!(r, Resource::Firewall(ports) if !ports.is_empty())));
    if !firewall {
        return None;
    }
    let catalogue = Catalogue::embedded().ok()?;
    let posix = catalogue.collections().get("ansible.posix")?;
    let collections = json!([{
        "name": posix.name,
        "version": posix.version,
        "source": posix.url.to_string(),
        "sha256": posix.sha256.to_string(),
    }]);
    let body = task::document(&json!({ "collections": collections }));
    Some(format!("{}{body}", header("#")))
}

#[cfg(test)]
mod tests;
