//! What Ansible needs to know about an environment, whatever the target: the inventory groups
//! (the `provision:` steps' `groups:` and the spec's own `groups:`) and each machine's
//! `host_vars`. How a target reaches its hosts (users, keys, WinRM) is the renderer's business
//! (`isoloom_render::inventory`).

use indexmap::IndexMap;
use isoloom_spec::groups;
use isoloom_spec::model::Spec;

/// The inventory groups: every `provision[].groups` entry (steps merged, in step order), then
/// the spec's `groups:` with their resolved members. A name in both lists its members once.
/// Members are machine names in the order first given; which of them a target can reach is
/// the renderer's filter.
pub fn groups(spec: &Spec) -> IndexMap<String, Vec<String>> {
    let mut out: IndexMap<String, Vec<String>> = IndexMap::new();
    let declared = spec.provision.iter().flat_map(|s| s.groups.iter().map(|(g, m)| (g.clone(), m.clone())));
    let resolved = spec.groups.keys().map(|g| (g.clone(), groups::members(spec, g)));
    for (group, members) in declared.chain(resolved) {
        let entry = out.entry(group).or_default();
        for m in members {
            if !entry.contains(&m) {
                entry.push(m);
            }
        }
    }
    out
}

/// The variables of `machine` from every `provision[].host_vars`, in step order (a later step
/// overrides an earlier one's value and keeps its place).
pub fn host_vars(spec: &Spec, machine: &str) -> IndexMap<String, serde_json::Value> {
    spec.provision
        .iter()
        .filter_map(|s| s.host_vars.get(machine))
        .flatten()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // tests: a failing test is the report.

    use super::*;

    const SPEC: &str = r"
version: 1
name: g
networks:
  lab: { cidr: 10.9.0.0/24 }
groups:
  web: { members: [w*] }
  all: { members: [web, db] }
machines:
  w1: { networks: { lab: 10 }, vm: { os: debian@12 } }
  w2: { networks: { lab: 11 }, vm: { os: debian@12 } }
  db: { networks: { lab: 20 }, vm: { os: debian@12 } }
provision:
  - ansible: a.yml
    groups: { web: [w2], dbs: [db] }
    host_vars: { db: { port: 5432, name: one } }
  - ansible: b.yml
    groups: { dbs: [db, w1] }
    host_vars: { db: { name: two } }
";

    fn spec() -> Spec {
        isoloom_spec::parse(SPEC).expect("parses")
    }

    #[test]
    fn provision_groups_come_first_then_the_specs_own_merged() {
        let g = groups(&spec());
        assert_eq!(g.keys().collect::<Vec<_>>(), ["web", "dbs", "all"]);
        assert_eq!(g["web"], ["w2", "w1"]);
        assert_eq!(g["dbs"], ["db", "w1"]);
        assert_eq!(g["all"], ["w1", "w2", "db"]);
    }

    #[test]
    fn host_vars_merge_across_steps_with_the_later_value() {
        let v = host_vars(&spec(), "db");
        assert_eq!(v.keys().collect::<Vec<_>>(), ["port", "name"]);
        assert_eq!(v["name"], serde_json::json!("two"));
        assert!(host_vars(&spec(), "w1").is_empty());
    }
}
