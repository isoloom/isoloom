//! `count:`: one machine written once, present several times. A workstation fleet:
//!
//! ```yaml
//! machines:
//!   ws:
//!     count: 5                 # ws-01 .. ws-05
//!     networks: { office: 50 } # .50 .. .54
//!     os: windows@11
//!     app: { steps: [{ run: provision/ws.ps1 }] }
//! ```
//!
//! The clones are ordinary machines named `<name>-01`, `<name>-02`, ..., each one address
//! further along on every network. Where the spec names the base machine, it means every
//! clone: `depends_on`, a check's `from`, a group's members and provisioning groups. Expansion happens on the YAML before the spec is parsed (before `common:`
//! and `groups:` fold in), so every reader sees finished machines.

use indexmap::IndexMap;
use serde_yaml_ng::{Mapping, Value};

use crate::syntax::{Code, FieldPath, Problem};

/// The highest `count`.
pub(crate) const MAX: u8 = 99;

fn key(s: &str) -> Value {
    Value::String(s.to_owned())
}

/// A clone's name.
#[must_use]
pub(crate) fn name(base: &str, index: u8) -> String {
    format!("{base}-{index:02}")
}

/// Base machine name -> its clones' names.
type Clones = IndexMap<String, Vec<String>>;

/// Expands every machine with a `count` into its clones, in place, and rewrites the places
/// that name the base machine. Returns base name -> clone names.
///
/// # Errors
///
/// Every `invalid-value` [`Problem`], each at its field: a `count` that isn't 2 to [`MAX`], a
/// count on the access machine, or a network's `gateway` or `switch` naming a counted machine.
pub(crate) fn expand(doc: &mut Value) -> Result<Clones, Vec<Problem>> {
    let Value::Mapping(top) = doc else { return Ok(Clones::new()) };
    let Some(Value::Mapping(machines)) = top.get(key("machines")).cloned() else {
        return Ok(Clones::new());
    };
    let (expanded, clones) = expand_machines(&machines)?;
    if clones.is_empty() {
        return Ok(clones);
    }
    top.insert(key("machines"), Value::Mapping(expanded));
    expand_members(top, &clones);
    expand_checks(top, &clones);
    let refused = one_machine_fields(top, &clones);
    if refused.is_empty() { Ok(clones) } else { Err(refused) }
}

/// The machines with each counted one replaced by its clones, and the clones of each.
fn expand_machines(machines: &Mapping) -> Result<(Mapping, Clones), Vec<Problem>> {
    let mut clones = Clones::new();
    let mut expanded = Mapping::new();
    let mut refused = Vec::new();
    for (k, v) in machines {
        let (Some(base), Value::Mapping(m)) = (k.as_str(), v) else {
            expanded.insert(k.clone(), v.clone());
            continue;
        };
        let count = match count_of(base, m) {
            Ok(Some(count)) => count,
            Ok(None) => {
                expanded.insert(k.clone(), v.clone());
                continue;
            }
            Err(problem) => {
                refused.push(problem);
                continue;
            }
        };
        if let Some(past) = past_the_last_octet(base, m, count) {
            refused.push(past);
            continue;
        }
        let mut names = Vec::new();
        for i in 1..=count {
            let n = name(base, i);
            expanded.insert(key(&n), Value::Mapping(clone_of(m, i)));
            names.push(n);
        }
        clones.insert(base.to_owned(), names);
    }
    if refused.is_empty() { Ok((expanded, clones)) } else { Err(refused) }
}

/// A machine's `count` (None without one), checked.
fn count_of(base: &str, m: &Mapping) -> Result<Option<u8>, Problem> {
    let refused = |reason: String| Problem::new(Code::InvalidValue, FieldPath::new("machines").key(base).key("count"), reason);
    let count = match m.get(key("count")) {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .filter(|n| (2..=MAX).contains(n))
            .ok_or_else(|| refused(format!("2 to {MAX} (one machine needs no count)")))?,
        Some(_) => return Err(refused(format!("a number of machines, 2 to {MAX}"))),
    };
    if matches!(m.get(key("access")), Some(Value::Bool(true))) {
        return Err(refused("the access machine is one machine; leave `count` out".to_owned()));
    }
    Ok(Some(count))
}

/// The problem when `count` clones of machine `base`, one address further each, run past an
/// address's last octet (.255) on a network, with the last address it would take.
fn past_the_last_octet(base: &str, m: &Mapping, count: u8) -> Option<Problem> {
    let Some(Value::Mapping(nets)) = m.get(key("networks")) else { return None };
    nets.iter().find_map(|(net, octets)| {
        let first = match octets {
            Value::Sequence(list) => list.iter().filter_map(Value::as_u64).max(),
            one => one.as_u64(),
        }?;
        let last = first + u64::from(count) - 1;
        (last > 255).then(|| {
            Problem::new(
                Code::InvalidValue,
                FieldPath::new("machines").key(base).key("networks").key(net.as_str().unwrap_or_default()),
                format!("{count} machines from .{first} run past .255 (the last would be .{last}): start lower or count fewer"),
            )
        })
    })
}

/// Clone `i` (from 1) of machine `m`: no `count`, each address `i - 1` further along.
fn clone_of(m: &Mapping, i: u8) -> Mapping {
    let mut clone = m.clone();
    clone.remove(key("count"));
    if let Some(Value::Mapping(nets)) = clone.get_mut(key("networks")) {
        let shift = |octet: &mut Value| {
            if let Some(o) = octet.as_u64() {
                *octet = Value::Number((o + u64::from(i) - 1).into());
            }
        };
        for (_, octets) in nets.iter_mut() {
            // Several addresses on the network (`[10, 11]`) all move along.
            match octets {
                Value::Sequence(list) => list.iter_mut().for_each(&shift),
                one => shift(one),
            }
        }
    }
    clone
}

/// A list of machine names with each base replaced by its clones.
fn expand_list(list: &mut Value, clones: &Clones) {
    if let Value::Sequence(items) = list {
        let mut out = Vec::new();
        for item in items.iter() {
            match item.as_str().and_then(|s| clones.get(s)) {
                Some(names) => out.extend(names.iter().map(|n| key(n))),
                None => out.push(item.clone()),
            }
        }
        *items = out;
    }
}

/// Where the base is named, every clone is meant: `depends_on`, a group's members, the
/// groups of a provisioning step.
fn expand_members(top: &mut Mapping, clones: &Clones) {
    if let Some(Value::Mapping(machines)) = top.get_mut(key("machines")) {
        for (_, m) in machines.iter_mut() {
            if let Value::Mapping(m) = m
                && let Some(deps) = m.get_mut(key("depends_on"))
            {
                expand_list(deps, clones);
            }
        }
    }
    if let Some(Value::Mapping(groups)) = top.get_mut(key("groups")) {
        for (_, g) in groups.iter_mut() {
            if let Value::Mapping(g) = g
                && let Some(members) = g.get_mut(key("members"))
            {
                expand_list(members, clones);
            }
        }
    }
    if let Some(Value::Sequence(steps)) = top.get_mut(key("provision")) {
        for step in steps.iter_mut() {
            if let Value::Mapping(step) = step
                && let Some(Value::Mapping(groups)) = step.get_mut(key("groups"))
            {
                for (_, members) in groups.iter_mut() {
                    expand_list(members, clones);
                }
            }
        }
    }
}

/// A check from the base runs from each clone.
fn expand_checks(top: &mut Mapping, clones: &Clones) {
    let Some(Value::Sequence(checks)) = top.get_mut(key("checks")) else { return };
    let mut out = Vec::new();
    for c in checks.iter() {
        match c {
            Value::Mapping(m) if m.get(key("from")).and_then(Value::as_str).is_some_and(|f| clones.contains_key(f)) => {
                let base = m.get(key("from")).and_then(Value::as_str).unwrap_or_default().to_owned();
                for n in clones.get(&base).into_iter().flatten() {
                    let mut each = m.clone();
                    each.insert(key("from"), key(n));
                    if let Some(Value::String(name)) = each.get_mut(key("name")) {
                        *name = format!("{name} ({n})");
                    }
                    out.push(Value::Mapping(each));
                }
            }
            other => out.push(other.clone()),
        }
    }
    *checks = out;
}

/// A gateway, or a LAN's switch, is one machine: every network's field that names a counted one.
fn one_machine_fields(top: &Mapping, clones: &Clones) -> Vec<Problem> {
    let Some(Value::Mapping(nets)) = top.get(key("networks")) else {
        return Vec::new();
    };
    let mut refused = Vec::new();
    for (n, net) in nets {
        let (Some(n), Value::Mapping(net)) = (n.as_str(), net) else { continue };
        for field in ["gateway", "switch"] {
            if let Some(m) = net.get(key(field)).and_then(Value::as_str)
                && clones.contains_key(m)
            {
                let at = FieldPath::new("networks").key(n).key(field);
                refused.push(Problem::new(Code::InvalidValue, at, format!("`{m}` has a count; a {field} is one machine")));
            }
        }
    }
    refused
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expanded(yaml: &str) -> (Value, Result<Clones, Vec<Problem>>) {
        let mut doc: Value = serde_yaml_ng::from_str(yaml).unwrap();
        let clones = expand(&mut doc);
        (doc, clones)
    }

    #[test]
    fn a_counted_machine_becomes_its_clones_one_address_further_each() {
        let (doc, clones) = expanded("machines:\n  ws: { count: 3, networks: { lan: 50, mgmt: [20, 30] } }\n  db: { networks: { lan: 10 } }\n");
        assert_eq!(clones.unwrap()["ws"], ["ws-01", "ws-02", "ws-03"]);
        let names: Vec<&str> = doc["machines"].as_mapping().unwrap().keys().filter_map(Value::as_str).collect();
        assert_eq!(names, ["ws-01", "ws-02", "ws-03", "db"]);
        assert_eq!(doc["machines"]["ws-03"]["networks"]["lan"].as_u64(), Some(52));
        assert_eq!(doc["machines"]["ws-02"]["networks"]["mgmt"][1].as_u64(), Some(31));
        assert!(doc["machines"]["ws-01"].get("count").is_none());
    }

    #[test]
    fn a_check_from_the_base_runs_from_each_clone() {
        let (doc, _) = expanded(
            "machines:\n  ws: { count: 2 }\n  db: {}\nchecks:\n  - { name: reaches the db, from: ws, tcp: 'db:5432' }\n  - { from: db, tcp: 'ws-01:22' }\n  - probe.sh\n",
        );
        let checks = doc["checks"].as_sequence().unwrap();
        assert_eq!(checks.len(), 4);
        assert_eq!(
            (checks[0]["from"].as_str(), checks[0]["name"].as_str()),
            (Some("ws-01"), Some("reaches the db (ws-01)"))
        );
        assert_eq!(
            (checks[1]["from"].as_str(), checks[1]["name"].as_str()),
            (Some("ws-02"), Some("reaches the db (ws-02)"))
        );
        assert_eq!(checks[2]["from"].as_str(), Some("db"));
        assert_eq!(checks[3].as_str(), Some("probe.sh"));
    }

    #[test]
    fn where_the_base_is_named_every_clone_is_meant() {
        let (doc, _) = expanded(
            "machines:\n  ws: { count: 2 }\n  app: { depends_on: [ws, db] }\n  db: {}\ngroups:\n  desk: { members: [ws] }\nprovision:\n  - { ansible: site.yml, groups: { clients: [ws] } }\n",
        );
        let list = |v: &Value| v.as_sequence().unwrap().iter().filter_map(Value::as_str).map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(list(&doc["machines"]["app"]["depends_on"]), ["ws-01", "ws-02", "db"]);
        assert_eq!(list(&doc["groups"]["desk"]["members"]), ["ws-01", "ws-02"]);
        assert_eq!(list(&doc["provision"][0]["groups"]["clients"]), ["ws-01", "ws-02"]);
    }

    #[test]
    fn a_gateway_or_a_switch_is_one_machine() {
        for field in ["gateway", "switch"] {
            let (_, refused) = expanded(&format!("networks:\n  lan: {{ {field}: rt }}\nmachines:\n  rt: {{ count: 2 }}\n"));
            let [p] = &refused.unwrap_err()[..] else { panic!("one problem") };
            assert_eq!((p.code, p.path.as_str()), (Code::InvalidValue, format!("networks.lan.{field}").as_str()));
            assert_eq!(p.reason, format!("`rt` has a count; a {field} is one machine"));
        }
        let (_, fine) = expanded("networks:\n  lan: { gateway: rt }\nmachines:\n  rt: {}\n  ws: { count: 2 }\n");
        assert_eq!(fine.unwrap()["ws"].len(), 2);
    }

    #[test]
    fn every_network_naming_a_counted_machine_is_reported() {
        let (_, refused) = expanded("networks:\n  a: { gateway: rt, switch: sw }\n  b: { gateway: rt }\nmachines:\n  rt: { count: 2 }\n  sw: { count: 2 }\n");
        let paths: Vec<String> = refused.unwrap_err().iter().map(|p| p.path.to_string()).collect();
        assert_eq!(paths, ["networks.a.gateway", "networks.a.switch", "networks.b.gateway"]);
    }

    #[test]
    fn clones_running_past_the_last_octet_are_one_problem_at_the_network() {
        let (_, refused) = expanded("machines:\n  ws: { count: 10, networks: { lan: 250 } }\n  db: { count: 3, networks: { lan: 20, mgmt: [200, 254] } }\n");
        let problems: Vec<(String, String)> = refused.unwrap_err().iter().map(|p| (p.path.to_string(), p.reason.clone())).collect();
        assert_eq!(
            problems,
            [
                (
                    "machines.ws.networks.lan".to_owned(),
                    "10 machines from .250 run past .255 (the last would be .259): start lower or count fewer".to_owned()
                ),
                (
                    "machines.db.networks.mgmt".to_owned(),
                    "3 machines from .254 run past .255 (the last would be .256): start lower or count fewer".to_owned()
                ),
            ]
        );
        let (_, fine) = expanded("machines:\n  ws: { count: 6, networks: { lan: 250 } }\n");
        assert_eq!(fine.unwrap()["ws"].len(), 6, "up to .255 is for the address checks to judge");
    }

    #[test]
    fn every_wrong_count_is_reported() {
        let (_, refused) = expanded("machines:\n  ws: { count: 1 }\n  db: { count: two }\n  me: { count: 2, access: true }\n");
        let paths: Vec<String> = refused.unwrap_err().iter().map(|p| p.path.to_string()).collect();
        assert_eq!(paths, ["machines.ws.count", "machines.db.count", "machines.me.count"]);
    }

    #[test]
    fn a_count_is_2_to_99_on_a_machine_that_isnt_the_access_one() {
        let refused = |yaml: &str| expanded(yaml).1.unwrap_err().remove(0).reason;
        assert_eq!(refused("machines:\n  ws: { count: 1 }\n"), "2 to 99 (one machine needs no count)");
        assert_eq!(refused("machines:\n  ws: { count: 100 }\n"), "2 to 99 (one machine needs no count)");
        assert_eq!(refused("machines:\n  ws: { count: two }\n"), "a number of machines, 2 to 99");
        assert_eq!(
            refused("machines:\n  me: { count: 2, access: true }\n"),
            "the access machine is one machine; leave `count` out"
        );
        assert_eq!(expanded("machines:\n  ws: { count: 99 }\n").1.unwrap()["ws"].len(), 99);
        assert_eq!(expanded("machines:\n  ws: { count: ~ }\n").1.unwrap().len(), 0);
    }
}
