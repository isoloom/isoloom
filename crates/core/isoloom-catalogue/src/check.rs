//! The catalogue's rules, checked when it loads: each entry is complete and consistent, its
//! recipe resolves on every release and OS with every setting set or unset, every setting does
//! something in every form, its files are its own (Apache-2.0), its contract names only what the
//! entry has, the daemon table names each daemon once, and the pinned Galaxy content is
//! Ansible's own. One [`Finding`] per broken rule, at its file and field.

mod contract;
mod definition;
mod samples;
mod usage;

use crate::catalogue::{Catalogue, Entry};
use crate::collections::NAMESPACE;
use crate::entry::Daemon;
use crate::error::Finding;

/// Every rule the catalogue breaks.
pub(crate) fn catalogue(c: &Catalogue) -> Vec<Finding> {
    let mut out = Vec::new();
    for entry in c.entries() {
        out.extend(one(c, entry));
    }
    out.extend(daemon_keys(c));
    for (key, d) in c.shared_daemons() {
        out.extend(daemon_rules(d).into_iter().map(|reason| Finding::new(format!("daemons.yml: {key}"), reason)));
    }
    for collection in &c.collections().collections {
        if collection.name.split_once('.').is_none_or(|(ns, _)| ns != NAMESPACE) {
            out.push(Finding::new(
                "collections.yml",
                format!(
                    "`{}` isn't Ansible's own content: only `{NAMESPACE}.*` collections are pinned (never `community.*` or a vendor's)",
                    collection.name
                ),
            ));
        }
    }
    out
}

/// Every rule entry `entry` breaks.
fn one(c: &Catalogue, entry: &Entry) -> Vec<Finding> {
    let mut out = definition::check(entry);
    out.extend(usage::check(entry));
    out.extend(contract::check(entry));
    // Resolving needs a consistent definition: its findings would only repeat these.
    if out.is_empty() {
        out.extend(samples::check(c, entry));
    }
    out
}

/// What daemon `d` breaks: its foreground form and its package exist on every family its unit
/// does.
fn daemon_rules(d: &Daemon) -> Vec<String> {
    let mut out = Vec::new();
    for family in d.unit.families() {
        if d.foreground.on(family).is_none() {
            out.push(format!("no foreground form on the {} family, where its unit is", family.id()));
        }
        if d.package.as_ref().is_some_and(|p| p.on(family).is_none()) {
            out.push(format!("no package on the {} family, where its unit is", family.id()));
        }
    }
    out
}

/// A daemon named twice in the table (two entries, or an entry and `daemons.yml`).
fn daemon_keys(c: &Catalogue) -> Vec<Finding> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for (key, _) in c.daemons() {
        if seen.contains(&key) {
            out.push(Finding::new(
                "daemons.yml",
                format!("the daemon `{key}` is defined twice: each daemon has one owner (its entry, else daemons.yml)"),
            ));
        }
        seen.push(key);
    }
    out
}
