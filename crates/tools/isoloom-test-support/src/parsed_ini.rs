//! A rendered Ansible inventory (INI), read as Ansible reads it.

use std::collections::BTreeMap;

/// The groups of inventory `text`, each with its lines (a host and its variables, or a
/// `[group:vars]` value), comments and blank lines left out, in order.
#[must_use]
pub fn ini_groups(text: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut group = String::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            name.clone_into(&mut group);
            out.entry(group.clone()).or_default();
        } else {
            out.entry(group.clone()).or_default().push(line.to_owned());
        }
    }
    out
}
