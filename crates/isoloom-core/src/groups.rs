//! Shared machine fields: `common:` for every machine, `groups:` for some. A spec that gives
//! five servers the same Windows block writes it once:
//!
//! ```yaml
//! common:
//!   vm: { os: debian-12 }
//!   resources: { cpus: 2, memory_mb: 2048 }
//! groups:
//!   servers:
//!     members: [dc01, dc02, srv02]        # machine names, globs (ws*), other groups
//!     vm: { os: windows-server-2019 }
//! ```
//!
//! Any field a group may share deep-merges into its members: a machine's own value wins over
//! its groups', a group's over `common:`, and between groups the most specific one (a group
//! that is a member of another) wins; otherwise the later declared. `docker:` and `vm:` only
//! complete an implementation the machine declares itself (even as `vm: {}`), so a shared
//! `vm:` never turns a machine into a VM it wasn't. Groups also become Ansible inventory
//! groups, and `isoloom exec <group>` runs on their members.
//!
//! Expansion happens on the YAML before the spec is parsed, so every reader of a `Spec` sees
//! finished machines; the typed `common` and `groups` stay in the spec for validation, the
//! inventory and the snapshot.

use indexmap::IndexMap;
use serde_yaml_ng::{Mapping, Value};

use crate::model::Spec;

/// Group names Isoloom fills itself in inventories.
pub const RESERVED: &[&str] = &["all", "linux", "windows"];

fn key(s: &str) -> Value {
    Value::String(s.to_string())
}

/// Whether `pattern` (a name, or a glob with `*`) matches `name`.
pub fn matches(pattern: &str, name: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == name;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut rest = name;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            let Some(r) = rest.strip_prefix(part) else { return false };
            rest = r;
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else if let Some(pos) = rest.find(part) {
            rest = &rest[pos + part.len()..];
        } else {
            return false;
        }
    }
    true
}

/// The machines of a group: its member names and globs, and the members of the groups it
/// names (recursively, cycles stopped). In spec order.
pub fn members(spec: &Spec, group: &str) -> Vec<String> {
    let mut seen_groups = Vec::new();
    let mut out: Vec<String> = Vec::new();
    collect(spec, group, &mut seen_groups, &mut out);
    spec.machines.keys().filter(|m| out.contains(m)).cloned().collect()
}

fn collect(spec: &Spec, group: &str, seen: &mut Vec<String>, out: &mut Vec<String>) {
    if seen.iter().any(|g| g == group) {
        return;
    }
    seen.push(group.to_string());
    let Some(g) = spec.groups.get(group) else { return };
    for pattern in &g.members {
        if spec.groups.contains_key(pattern) && !spec.machines.contains_key(pattern) {
            collect(spec, pattern, seen, out);
            continue;
        }
        for m in spec.machines.keys() {
            if matches(pattern, m) && !out.contains(m) {
                out.push(m.clone());
            }
        }
    }
}

/// Expands `common:` and `groups:` into the machines of a spec document (the YAML, parsed).
/// Unknown members are left for validation to report.
pub fn expand(doc: &mut Value) -> Result<(), String> {
    let Value::Mapping(top) = doc else { return Ok(()) };
    let common = match top.get(key("common")) {
        Some(Value::Mapping(m)) => m.clone(),
        Some(Value::Null) | None => Mapping::new(),
        Some(_) => return Err("common: expected a mapping of machine fields".into()),
    };
    let groups: IndexMap<String, Mapping> = match top.get(key("groups")) {
        Some(Value::Mapping(m)) => m
            .iter()
            .map(|(k, v)| match v {
                Value::Mapping(g) => Ok((k.as_str().unwrap_or_default().to_string(), g.clone())),
                _ => Err(format!(
                    "groups.{}: expected a mapping with `members` and machine fields",
                    k.as_str().unwrap_or_default()
                )),
            })
            .collect::<Result<_, _>>()?,
        Some(Value::Null) | None => IndexMap::new(),
        Some(_) => return Err("groups: expected a mapping of group name to group".into()),
    };
    if common.is_empty() && groups.is_empty() {
        return Ok(());
    }
    let Some(Value::Mapping(machines)) = top.get(key("machines")).cloned() else {
        return Ok(());
    };
    let names: Vec<String> = machines.keys().filter_map(|k| k.as_str().map(str::to_string)).collect();

    // Each group's machines (globs and nested groups resolved).
    fn resolve(groups: &IndexMap<String, Mapping>, names: &[String], group: &str, seen: &mut Vec<String>, out: &mut Vec<String>) {
        if seen.iter().any(|g| g == group) {
            return;
        }
        seen.push(group.to_string());
        let Some(g) = groups.get(group) else { return };
        let Some(Value::Sequence(members)) = g.get(key("members")) else { return };
        for m in members.iter().filter_map(Value::as_str) {
            if groups.contains_key(m) && !names.iter().any(|n| n == m) {
                resolve(groups, names, m, seen, out);
            } else {
                for n in names {
                    if matches(m, n) && !out.contains(n) {
                        out.push(n.clone());
                    }
                }
            }
        }
    }
    let mut machines_of: IndexMap<&str, Vec<String>> = IndexMap::new();
    let mut nested_in: IndexMap<&str, Vec<&str>> = IndexMap::new();
    for g in groups.keys() {
        let mut out = Vec::new();
        resolve(&groups, &names, g, &mut Vec::new(), &mut out);
        machines_of.insert(g, out);
        // The groups this one is (transitively) a member of: its ancestors.
        let ancestors: Vec<&str> = groups
            .keys()
            .filter(|other| *other != g)
            .filter(|other| {
                let mut seen = Vec::new();
                contains_group(&groups, other, g, &mut seen)
            })
            .map(String::as_str)
            .collect();
        nested_in.insert(g, ancestors);
    }

    let mut expanded = Mapping::new();
    for (k, v) in &machines {
        let Some(name) = k.as_str() else {
            expanded.insert(k.clone(), v.clone());
            continue;
        };
        let own = match v {
            Value::Mapping(m) => m.clone(),
            _ => {
                expanded.insert(k.clone(), v.clone());
                continue;
            }
        };
        // The groups this machine is in, least specific first: ancestors before their members,
        // declaration order otherwise.
        let mut mine: Vec<&str> = groups
            .keys()
            .map(String::as_str)
            .filter(|g| machines_of[*g].iter().any(|m| m == name))
            .collect();
        let in_mine = mine.clone();
        mine.sort_by_key(|g| nested_in[*g].iter().filter(|a| in_mine.contains(a)).count());
        let mut merged = Value::Mapping(Mapping::new());
        let layers = std::iter::once(&common).chain(mine.iter().map(|g| &groups[*g]));
        for layer in layers {
            let mut shared = layer.clone();
            shared.remove(key("members"));
            // An implementation is only completed, never added.
            for imp in ["docker", "vm"] {
                if !own.contains_key(key(imp)) {
                    shared.remove(key(imp));
                }
            }
            crate::defaults::merge(&mut merged, &Value::Mapping(shared));
        }
        crate::defaults::merge(&mut merged, &Value::Mapping(own));
        expanded.insert(k.clone(), merged);
    }
    top.insert(key("machines"), Value::Mapping(expanded));
    Ok(())
}

/// Whether `group` has `target` among its members, directly or through other groups.
fn contains_group(groups: &IndexMap<String, Mapping>, group: &str, target: &str, seen: &mut Vec<String>) -> bool {
    if seen.iter().any(|g| g == group) {
        return false;
    }
    seen.push(group.to_string());
    let Some(g) = groups.get(group) else { return false };
    let Some(Value::Sequence(members)) = g.get(key("members")) else {
        return false;
    };
    members
        .iter()
        .filter_map(Value::as_str)
        .any(|m| m == target || (groups.contains_key(m) && contains_group(groups, m, target, seen)))
}

/// The groups' problems: a reserved or taken name, a member that names nothing, a cycle.
pub fn problems(spec: &Spec) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (name, g) in &spec.groups {
        let at = format!("groups.{name}");
        if RESERVED.contains(&name.as_str()) {
            out.push((at.clone(), "Isoloom fills this group itself; choose another name".into()));
        }
        if spec.machines.contains_key(name) {
            out.push((at.clone(), format!("`{name}` is already a machine's name")));
        }
        if g.members.is_empty() {
            out.push((format!("{at}.members"), "name the machines in the group".into()));
        }
        for (i, m) in g.members.iter().enumerate() {
            if m == name && !spec.machines.contains_key(m) {
                out.push((format!("{at}.members[{i}]"), "a group can't be its own member".into()));
            } else if !spec.groups.contains_key(m) && !spec.machines.keys().any(|n| matches(m, n)) {
                out.push((format!("{at}.members[{i}]"), format!("`{m}` names no machine or group")));
            }
        }
        let mut seen = Vec::new();
        let groups_raw: IndexMap<String, Mapping> = spec
            .groups
            .iter()
            .map(|(k, v)| {
                let mut m = Mapping::new();
                // Only members that name groups (and not machines) can form a loop.
                let group_members = v.members.iter().filter(|s| spec.groups.contains_key(*s) && !spec.machines.contains_key(*s));
                m.insert(key("members"), Value::Sequence(group_members.map(|s| key(s)).collect()));
                (k.clone(), m)
            })
            .collect();
        if contains_group(&groups_raw, name, name, &mut seen) {
            out.push((format!("{at}.members"), "groups contain each other in a loop".into()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_match_prefixes_suffixes_and_middles() {
        assert!(matches("ws*", "ws01") && !matches("ws*", "dc01"));
        assert!(matches("*01", "ws01") && matches("w*1", "ws01") && !matches("w*2", "ws01"));
        assert!(matches("dc01", "dc01") && !matches("dc0", "dc01"));
    }
}
