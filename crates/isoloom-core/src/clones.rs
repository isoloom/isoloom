//! `count:`: one machine written once, present several times. A workstation fleet:
//!
//! ```yaml
//! machines:
//!   ws:
//!     count: 5                 # ws-01 .. ws-05
//!     networks: { office: 50 } # .50 .. .54
//!     vm: { os: windows-11, provision: [provision/ws.ps1] }
//! ```
//!
//! The clones are ordinary machines named `<name>-01`, `<name>-02`, ..., each one address
//! further along on every network. Where the spec names the base machine, it means every
//! clone: `depends_on`, a check's `from`, a group's members, provisioning groups, and
//! `isoloom exec`. Expansion happens on the YAML before the spec is parsed (before `common:`
//! and `groups:` fold in), so every reader sees finished machines.

use indexmap::IndexMap;
use serde_yaml_ng::{Mapping, Value};

/// The highest `count`.
pub const MAX: u8 = 99;

fn key(s: &str) -> Value {
    Value::String(s.to_string())
}

/// A clone's name.
pub fn name(base: &str, index: u8) -> String {
    format!("{base}-{index:02}")
}

/// Expands every machine with a `count` into its clones, in place, and rewrites the places
/// that name the base machine. Returns base name -> clone names.
pub fn expand(doc: &mut Value) -> Result<IndexMap<String, Vec<String>>, String> {
    let mut clones: IndexMap<String, Vec<String>> = IndexMap::new();
    let Value::Mapping(top) = doc else { return Ok(clones) };
    let Some(Value::Mapping(machines)) = top.get(key("machines")).cloned() else {
        return Ok(clones);
    };
    let mut expanded = Mapping::new();
    for (k, v) in &machines {
        let (Some(base), Value::Mapping(m)) = (k.as_str(), v) else {
            expanded.insert(k.clone(), v.clone());
            continue;
        };
        let count = match m.get(key("count")) {
            None | Some(Value::Null) => {
                expanded.insert(k.clone(), v.clone());
                continue;
            }
            Some(Value::Number(n)) => n
                .as_u64()
                .filter(|n| (2..=u64::from(MAX)).contains(n))
                .ok_or_else(|| format!("machines.{base}.count: 2 to {MAX} (one machine needs no count)"))?,
            Some(_) => return Err(format!("machines.{base}.count: a number of machines, 2 to {MAX}")),
        };
        if matches!(m.get(key("access")), Some(Value::Bool(true))) {
            return Err(format!("machines.{base}.count: the access machine is one machine; leave `count` out"));
        }
        let mut names = Vec::new();
        for i in 1..=count as u8 {
            let mut clone = m.clone();
            clone.remove(key("count"));
            if let Some(Value::Mapping(nets)) = clone.get_mut(key("networks")) {
                for (_, octet) in nets.iter_mut() {
                    if let Some(o) = octet.as_u64() {
                        *octet = Value::Number((o + u64::from(i) - 1).into());
                    }
                }
            }
            let n = name(base, i);
            expanded.insert(key(&n), Value::Mapping(clone));
            names.push(n);
        }
        clones.insert(base.to_string(), names);
    }
    if clones.is_empty() {
        return Ok(clones);
    }
    top.insert(key("machines"), Value::Mapping(expanded));

    // Where the base is named, every clone is meant.
    let expand_list = |list: &mut Value| {
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
    };
    if let Some(Value::Mapping(machines)) = top.get_mut(key("machines")) {
        for (_, m) in machines.iter_mut() {
            if let Value::Mapping(m) = m
                && let Some(deps) = m.get_mut(key("depends_on"))
            {
                expand_list(deps);
            }
        }
    }
    if let Some(Value::Mapping(groups)) = top.get_mut(key("groups")) {
        for (_, g) in groups.iter_mut() {
            if let Value::Mapping(g) = g
                && let Some(members) = g.get_mut(key("members"))
            {
                expand_list(members);
            }
        }
    }
    if let Some(Value::Sequence(steps)) = top.get_mut(key("provision")) {
        for step in steps.iter_mut() {
            if let Value::Mapping(step) = step
                && let Some(Value::Mapping(groups)) = step.get_mut(key("groups"))
            {
                for (_, members) in groups.iter_mut() {
                    expand_list(members);
                }
            }
        }
    }
    // A check from the base runs from each clone.
    if let Some(Value::Sequence(checks)) = top.get_mut(key("checks")) {
        let mut out = Vec::new();
        for c in checks.iter() {
            match c {
                Value::Mapping(m) if m.get(key("from")).and_then(Value::as_str).is_some_and(|f| clones.contains_key(f)) => {
                    let base = m.get(key("from")).and_then(Value::as_str).unwrap_or_default().to_string();
                    for n in &clones[&base] {
                        let mut each = m.clone();
                        each.insert(key("from"), key(n));
                        if let Some(Value::String(name)) = each.get_mut(key("name")) {
                            name.push_str(&format!(" ({n})"));
                        }
                        out.push(Value::Mapping(each));
                    }
                }
                other => out.push(other.clone()),
            }
        }
        *checks = out;
    }
    // A gateway is one machine.
    if let Some(Value::Mapping(nets)) = top.get(key("networks")) {
        for (n, net) in nets {
            if let Value::Mapping(net) = net
                && let Some(gw) = net.get(key("gateway")).and_then(Value::as_str)
                && clones.contains_key(gw)
            {
                return Err(format!(
                    "networks.{}.gateway: `{gw}` has a count; a gateway is one machine",
                    n.as_str().unwrap_or_default()
                ));
            }
        }
    }
    Ok(clones)
}
