//! Shared machine fields: `common:` for every machine, `groups:` for some. A spec that gives
//! five servers the same Windows block writes it once:
//!
//! ```yaml
//! common:
//!   vm: { os: debian@12 }
//!   resources: { cpus: 2, memory_mb: 2048 }
//! groups:
//!   servers:
//!     members: [dc01, dc02, srv02]        # machine names, globs (ws*), other groups
//!     vm: { os: windows-server@2019 }
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
use crate::syntax::{Code, FieldPath, Problem};

/// Group names Isoloom fills itself in inventories.
pub(crate) const RESERVED: &[&str] = &["all", "linux", "windows"];

/// Whether `pattern` (a name, or a glob with `*`) matches `name`.
#[must_use]
pub fn matches(pattern: &str, name: &str) -> bool {
    let mut parts = pattern.split('*');
    let Some(mut rest) = parts.next().and_then(|first| name.strip_prefix(first)) else {
        return false;
    };
    let Some(last) = parts.next_back() else {
        return rest.is_empty();
    };
    for middle in parts {
        match rest.split_once(middle) {
            Some((_, after)) => rest = after,
            None => return false,
        }
    }
    rest.ends_with(last)
}

/// The groups of a spec and its machines' names: what membership is resolved against, whether
/// the groups come from the parsed spec or from its YAML.
struct Membership<'a> {
    /// Each group's members as written: machine names, globs, other groups.
    groups: IndexMap<&'a str, Vec<&'a str>>,
    /// The machines, in spec order.
    machines: Vec<&'a str>,
}

impl<'a> Membership<'a> {
    /// The groups and machines of a parsed spec.
    fn of_spec(spec: &'a Spec) -> Membership<'a> {
        Membership {
            groups: spec
                .groups
                .iter()
                .map(|(name, g)| (name.as_str(), g.members.iter().map(String::as_str).collect()))
                .collect(),
            machines: spec.machines.keys().map(String::as_str).collect(),
        }
    }

    /// Whether `name` is a group, and not also a machine (a machine's name wins).
    fn is_group(&self, name: &str) -> bool {
        self.groups.contains_key(name) && !self.machines.contains(&name)
    }

    /// The machines of `group`: its member names and globs, and the members of the groups it
    /// names (recursively, cycles stopped), in spec order.
    fn machines_of(&self, group: &str) -> Vec<&'a str> {
        let mut seen = Vec::new();
        let mut found = Vec::new();
        self.collect(group, &mut seen, &mut found);
        self.machines.iter().copied().filter(|m| found.contains(m)).collect()
    }

    fn collect(&self, group: &str, seen: &mut Vec<String>, found: &mut Vec<&'a str>) {
        if seen.iter().any(|g| g == group) {
            return;
        }
        seen.push(group.to_owned());
        for pattern in self.groups.get(group).into_iter().flatten() {
            if self.is_group(pattern) {
                self.collect(pattern, seen, found);
            } else {
                found.extend(self.machines.iter().copied().filter(|m| matches(pattern, m)));
            }
        }
    }

    /// Whether `group` holds group `target`, directly or through other groups (a member named
    /// like a machine is the machine).
    fn contains(&self, group: &str, target: &str, seen: &mut Vec<String>) -> bool {
        if seen.iter().any(|g| g == group) {
            return false;
        }
        seen.push(group.to_owned());
        self.groups
            .get(group)
            .into_iter()
            .flatten()
            .any(|m| self.is_group(m) && (*m == target || self.contains(m, target, seen)))
    }

    /// The groups a machine is in, least specific first: a group before the groups inside it,
    /// declaration order otherwise.
    fn layers_for(&self, machine: &str) -> Vec<&'a str> {
        let mine: Vec<&'a str> = self.groups.keys().copied().filter(|g| self.machines_of(g).contains(&machine)).collect();
        // A group's depth: how many of the machine's other groups hold it.
        let depth = |g: &str| mine.iter().filter(|other| **other != g && self.contains(other, g, &mut Vec::new())).count();
        let mut ranked: Vec<(usize, &'a str)> = mine.iter().map(|g| (depth(g), *g)).collect();
        ranked.sort_by_key(|(d, _)| *d);
        ranked.into_iter().map(|(_, g)| g).collect()
    }
}

/// The machines of a group: its member names and globs, and the members of the groups it
/// names (recursively, cycles stopped). In spec order.
#[must_use]
pub fn members(spec: &Spec, group: &str) -> Vec<String> {
    Membership::of_spec(spec).machines_of(group).into_iter().map(str::to_owned).collect()
}

/// Folds `common:` and `groups:` into the machines of a spec document (the YAML, parsed): a
/// machine's own value wins over its groups' (the most specific first), and theirs over
/// `common:`. Unknown members are left for validation to report.
///
/// # Errors
///
/// An `invalid-value` [`Problem`] when `common:`, `groups:` or a group isn't a mapping.
pub fn expand(doc: &mut Value) -> Result<(), Problem> {
    let Value::Mapping(top) = doc else { return Ok(()) };
    let common = common_of(top)?;
    let groups = groups_of(top)?;
    if common.is_empty() && groups.is_empty() {
        return Ok(());
    }
    let Some(Value::Mapping(machines)) = top.get("machines").cloned() else {
        return Ok(());
    };
    let membership = Membership {
        groups: groups.iter().map(|(name, g)| (name.as_str(), members_written(g))).collect(),
        machines: machines.keys().filter_map(Value::as_str).collect(),
    };
    let mut expanded = Mapping::new();
    for (k, v) in &machines {
        let merged = match (k.as_str(), v) {
            (Some(name), Value::Mapping(own)) => merged(own, &common, &membership.layers_for(name), &groups),
            _ => v.clone(),
        };
        expanded.insert(k.clone(), merged);
    }
    top.insert("machines".into(), Value::Mapping(expanded));
    Ok(())
}

/// The groups of a spec document, by name (as written: members and shared fields).
type Groups = IndexMap<String, Mapping>;

/// A group's `members` as written in the YAML.
fn members_written(group: &Mapping) -> Vec<&str> {
    match group.get("members") {
        Some(Value::Sequence(members)) => members.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

/// `common:`, empty when left out.
fn common_of(top: &Mapping) -> Result<Mapping, Problem> {
    match top.get("common") {
        Some(Value::Mapping(m)) => Ok(m.clone()),
        Some(Value::Null) | None => Ok(Mapping::new()),
        Some(_) => Err(Problem::new(
            Code::InvalidValue,
            FieldPath::new("common"),
            "expected a mapping of machine fields",
        )),
    }
}

/// `groups:`, empty when left out.
fn groups_of(top: &Mapping) -> Result<Groups, Problem> {
    match top.get("groups") {
        Some(Value::Mapping(m)) => m
            .iter()
            .map(|(k, v)| {
                let name = k.as_str().unwrap_or_default();
                match v {
                    Value::Mapping(g) => Ok((name.to_owned(), g.clone())),
                    _ => Err(Problem::new(
                        Code::InvalidValue,
                        FieldPath::new("groups").key(name),
                        "expected a mapping with `members` and machine fields",
                    )),
                }
            })
            .collect(),
        Some(Value::Null) | None => Ok(Groups::new()),
        Some(_) => Err(Problem::new(
            Code::InvalidValue,
            FieldPath::new("groups"),
            "expected a mapping of group name to group",
        )),
    }
}

/// A machine's own fields over its groups' (least specific first) over `common:`.
fn merged(own: &Mapping, common: &Mapping, mine: &[&str], groups: &Groups) -> Value {
    let mut merged = Value::Mapping(Mapping::new());
    let layers = std::iter::once(common).chain(mine.iter().filter_map(|g| groups.get(*g)));
    for layer in layers {
        let mut shared = layer.clone();
        shared.remove("members");
        crate::defaults::merge(&mut merged, &Value::Mapping(shared));
    }
    crate::defaults::merge(&mut merged, &Value::Mapping(own.clone()));
    merged
}

/// The groups' problems: a reserved or taken name, a member that names nothing, a cycle.
#[must_use]
pub fn problems(spec: &Spec) -> Vec<(String, String)> {
    let membership = Membership::of_spec(spec);
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
        if membership.contains(name, name, &mut Vec::new()) {
            out.push((format!("{at}.members"), "groups contain each other in a loop".into()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expanded(yaml: &str) -> Value {
        let mut doc: Value = serde_yaml_ng::from_str(yaml).unwrap();
        expand(&mut doc).unwrap();
        doc
    }

    #[test]
    fn a_machines_own_value_wins_over_its_groups_and_theirs_over_common() {
        let doc = expanded(
            "common: { resources: { cpus: 1, memory_mb: 512 }, privileged: true }\ngroups:\n  big: { members: [web], resources: { cpus: 4 } }\nmachines:\n  web: { resources: { memory_mb: 2048 } }\n  db: {}\n",
        );
        let web = &doc["machines"]["web"];
        assert_eq!(
            (web["resources"]["cpus"].as_u64(), web["resources"]["memory_mb"].as_u64()),
            (Some(4), Some(2048))
        );
        assert_eq!(web["privileged"].as_bool(), Some(true));
        assert_eq!(doc["machines"]["db"]["resources"]["cpus"].as_u64(), Some(1));
    }

    #[test]
    fn the_most_specific_group_wins() {
        let doc = expanded(
            "groups:\n  inner: { members: [web], os: debian@12 }\n  outer: { members: [inner], os: alpine, privileged: true }\nmachines:\n  web: {}\n",
        );
        let web = &doc["machines"]["web"];
        assert_eq!((web["os"].as_str(), web["privileged"].as_bool()), (Some("debian@12"), Some(true)));
    }

    #[test]
    fn shared_fields_of_the_wrong_shape_are_refused_at_their_field() {
        let mut common: Value = serde_yaml_ng::from_str("common: 5\nmachines: { a: {} }\n").unwrap();
        assert_eq!(expand(&mut common).unwrap_err().path.as_str(), "common");
        let mut group: Value = serde_yaml_ng::from_str("groups: { g: 5 }\nmachines: { a: {} }\n").unwrap();
        assert_eq!(expand(&mut group).unwrap_err().path.as_str(), "groups.g");
        let mut groups: Value = serde_yaml_ng::from_str("groups: [g]\nmachines: { a: {} }\n").unwrap();
        assert_eq!(expand(&mut groups).unwrap_err().path.as_str(), "groups");
    }

    #[test]
    fn a_groups_machines_follow_globs_and_nested_groups_in_spec_order() {
        let spec = crate::testing::model(
            "version: 1\nname: t\ngroups:\n  ws: { members: [ws*] }\n  all-hosts: { members: [ws, dc01] }\n  self: { members: [self, ws] }\nmachines:\n  dc01: {}\n  ws02: {}\n  ws01: {}\n",
        )
        .unwrap();
        assert_eq!(members(&spec, "all-hosts"), ["dc01", "ws02", "ws01"]);
        assert_eq!(members(&spec, "self"), ["ws02", "ws01"], "a group holding itself stops there");
        assert_eq!(members(&spec, "nope"), Vec::<String>::new());
    }

    #[test]
    fn group_loops_count_groups_not_machines_of_the_same_name() {
        let looped = crate::testing::model("version: 1\nname: t\ngroups:\n  a: { members: [b] }\n  b: { members: [a] }\nmachines:\n  m: {}\n").unwrap();
        assert!(problems(&looped).iter().any(|(at, why)| at == "groups.a.members" && why.contains("loop")));
        let named = crate::testing::model("version: 1\nname: t\ngroups:\n  m: { members: [m] }\nmachines:\n  m: {}\n").unwrap();
        let found = problems(&named);
        assert_eq!(found, [("groups.m".to_owned(), "`m` is already a machine's name".to_owned())]);
    }

    #[test]
    fn a_member_is_a_machine_a_glob_or_a_group_never_the_group_itself() {
        let spec = crate::testing::model(
            "version: 1\nname: t\ngroups:\n  g: { members: [g, ghost, ws*, h, web] }\n  h: { members: [web] }\nmachines:\n  web: {}\n  ws01: {}\n",
        )
        .unwrap();
        assert_eq!(
            problems(&spec),
            [
                ("groups.g.members[0]".to_owned(), "a group can't be its own member".to_owned()),
                ("groups.g.members[1]".to_owned(), "`ghost` names no machine or group".to_owned()),
                ("groups.g.members".to_owned(), "groups contain each other in a loop".to_owned()),
            ]
        );
    }

    #[test]
    fn globs_match_prefixes_suffixes_and_middles() {
        assert!(matches("ws*", "ws01") && !matches("ws*", "dc01"));
        assert!(matches("*01", "ws01") && matches("w*1", "ws01") && !matches("w*2", "ws01"));
        assert!(matches("dc01", "dc01") && !matches("dc0", "dc01"));
    }
}
