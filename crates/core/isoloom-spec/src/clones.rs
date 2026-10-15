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
//! clone: `depends_on`, a check's `from`, a group's members, provisioning groups, and
//! `isoloom exec`. Expansion happens on the YAML before the spec is parsed (before `common:`
//! and `groups:` fold in), so every reader sees finished machines.

use indexmap::IndexMap;
use serde_yaml_ng::{Mapping, Value};

use crate::syntax::{Code, FieldPath, Problem};

/// The highest `count`.
pub const MAX: u8 = 99;

fn key(s: &str) -> Value {
    Value::String(s.to_owned())
}

/// A clone's name.
#[must_use]
pub fn name(base: &str, index: u8) -> String {
    format!("{base}-{index:02}")
}

/// Base machine name -> its clones' names.
type Clones = IndexMap<String, Vec<String>>;

/// Expands every machine with a `count` into its clones, in place, and rewrites the places
/// that name the base machine. Returns base name -> clone names.
///
/// # Errors
///
/// An `invalid-value` [`Problem`] at the field: a `count` that isn't 2 to [`MAX`], a count on
/// the access machine, or a network's `gateway` or `switch` naming a counted machine.
pub fn expand(doc: &mut Value) -> Result<Clones, Problem> {
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
    one_machine_fields(top, &clones)?;
    Ok(clones)
}

/// The machines with each counted one replaced by its clones, and the clones of each.
fn expand_machines(machines: &Mapping) -> Result<(Mapping, Clones), Problem> {
    let mut clones = Clones::new();
    let mut expanded = Mapping::new();
    for (k, v) in machines {
        let (Some(base), Value::Mapping(m)) = (k.as_str(), v) else {
            expanded.insert(k.clone(), v.clone());
            continue;
        };
        let Some(count) = count_of(base, m)? else {
            expanded.insert(k.clone(), v.clone());
            continue;
        };
        let mut names = Vec::new();
        for i in 1..=count {
            let n = name(base, i);
            expanded.insert(key(&n), Value::Mapping(clone_of(m, i)));
            names.push(n);
        }
        clones.insert(base.to_owned(), names);
    }
    Ok((expanded, clones))
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

/// A gateway, or a LAN's switch, is one machine.
fn one_machine_fields(top: &Mapping, clones: &Clones) -> Result<(), Problem> {
    let Some(Value::Mapping(nets)) = top.get(key("networks")) else {
        return Ok(());
    };
    for (n, net) in nets {
        for field in ["gateway", "switch"] {
            if let Value::Mapping(net) = net
                && let Some(m) = net.get(key(field)).and_then(Value::as_str)
                && clones.contains_key(m)
            {
                let at = FieldPath::new("networks").key(n.as_str().unwrap_or_default()).key(field);
                return Err(Problem::new(Code::InvalidValue, at, format!("`{m}` has a count; a {field} is one machine")));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expanded(yaml: &str) -> (Value, Result<Clones, Problem>) {
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
            let p = refused.unwrap_err();
            assert_eq!((p.code, p.path.as_str()), (Code::InvalidValue, format!("networks.lan.{field}").as_str()));
            assert_eq!(p.reason, format!("`rt` has a count; a {field} is one machine"));
        }
        let (_, fine) = expanded("networks:\n  lan: { gateway: rt }\nmachines:\n  rt: {}\n  ws: { count: 2 }\n");
        assert_eq!(fine.unwrap()["ws"].len(), 2);
    }

    #[test]
    fn a_count_is_2_to_99_on_a_machine_that_isnt_the_access_one() {
        let refused = |yaml: &str| expanded(yaml).1.unwrap_err().reason;
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
