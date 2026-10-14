//! The field paths a spec document uses, the way the table names its rows.

use serde_yaml_ng::Value;

/// Fields counted as one feature whatever they hold.
const WHOLE: &[&str] = &[
    "common",
    "groups",
    "tools",
    "provision[].host_vars",
    "controller",
    // Maps of the author's own names (variables, kernel parameters, limits): one feature each.
    "machines.*.environment",
    "machines.*.sysctls",
    "machines.*.ulimits",
];

/// Where in the spec names are chosen by the author (map keys become `*`).
const NAMED: &[&str] = &[
    "networks",
    "networks.*.vlans",
    "machines",
    "machines.*.networks",
    "machines.*.volumes",
    "volumes",
    "cloud.vars",
    "cloud.outputs",
    "cloud.env",
    "checks[].headers",
    "machines.*.docker.args",
    "provision[].groups",
    "provision[].vars",
];

/// The field paths present in a spec document (`machines.*.docker.image`, `reach[].ports`).
/// A map of plain values under author-chosen names (`machines.*.networks`) counts as one field.
pub fn paths(doc: &Value) -> Vec<String> {
    fn walk(v: &Value, at: &str, out: &mut Vec<String>) {
        // Shared machine fields are one feature, however many fields they carry.
        if WHOLE.contains(&at) {
            out.push(at.to_string());
            return;
        }
        match v {
            Value::Mapping(m) if NAMED.contains(&at) => {
                if m.values().all(|c| !matches!(c, Value::Mapping(_))) {
                    out.push(at.to_string());
                } else {
                    for c in m.values() {
                        walk(c, &format!("{at}.*"), out);
                    }
                }
            }
            Value::Mapping(m) => {
                for (k, c) in m {
                    let k = k.as_str().unwrap_or_default();
                    walk(c, &if at.is_empty() { k.to_string() } else { format!("{at}.{k}") }, out);
                }
            }
            Value::Sequence(items) if items.iter().any(|i| matches!(i, Value::Mapping(_))) => {
                for i in items {
                    walk(i, &format!("{at}[]"), out);
                }
            }
            _ => out.push(at.to_string()),
        }
    }
    let mut out = Vec::new();
    walk(doc, "", &mut out);
    out.sort();
    out.dedup();
    out
}
