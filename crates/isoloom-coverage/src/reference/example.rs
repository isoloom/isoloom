//! A field's example: its probe spec, cut down to the part of the spec the field is in, written
//! back as one line of flow YAML (`reach: [{ from: lan, to: dmz, ports: [80] }]`).

use serde_yaml_ng::{Mapping, Value};

/// The part of `fragment` (a probe, a YAML flow mapping) under the field's top-level key, as
/// one line. The fragment as written when it isn't a mapping holding that key.
pub(super) fn focus(fragment: &str, path: &str) -> String {
    let fragment = fragment.trim();
    let top = path.split(['.', '[']).next().unwrap_or(path);
    let Ok(Value::Mapping(map)) = serde_yaml_ng::from_str::<Value>(fragment) else {
        return fragment.to_string();
    };
    match map.get(top) {
        Some(Value::Mapping(machines)) if top == "machines" => format!("machines: {}", mapping(&own_field(machines, path))),
        Some(v) => format!("{}: {}", scalar(top), flow(v)),
        None => fragment.to_string(),
    }
}

/// For a machine's field, the machines that set it, each with that field alone (a probe's
/// second machine carries everything a machine needs, which says nothing about the field).
fn own_field(machines: &Mapping, path: &str) -> Mapping {
    let Some(field) = path.strip_prefix("machines.*.").and_then(|rest| rest.split(['.', '[']).next()) else {
        return machines.clone();
    };
    let kept: Mapping = machines
        .iter()
        .filter_map(|(name, m)| {
            let v = m.get(field)?;
            let mut only = Mapping::new();
            only.insert(Value::String(field.to_string()), v.clone());
            Some((name.clone(), Value::Mapping(only)))
        })
        .collect();
    if kept.is_empty() { machines.clone() } else { kept }
}

/// A value as flow YAML.
fn flow(v: &Value) -> String {
    match v {
        Value::Mapping(m) => mapping(m),
        Value::Sequence(items) => format!("[{}]", items.iter().map(flow).collect::<Vec<_>>().join(", ")),
        Value::String(s) => scalar(s),
        Value::Tagged(t) => flow(&t.value),
        other => serde_yaml_ng::to_string(other).map(|s| s.trim().to_string()).unwrap_or_default(),
    }
}

/// A mapping as `{ key: value, ... }`.
fn mapping(m: &Mapping) -> String {
    if m.is_empty() {
        return "{}".to_string();
    }
    let pairs: Vec<String> = m.iter().map(|(k, v)| format!("{}: {}", flow(k), flow(v))).collect();
    format!("{{ {} }}", pairs.join(", "))
}

/// A string, quoted only when YAML would read it otherwise (a number, a flow indicator).
fn scalar(s: &str) -> String {
    let special =
        s.is_empty() || s.contains([',', '[', ']', '{', '}', '#', '\'', '"', ':']) || s.starts_with(['&', '*', '!', '|', '>', '%', '@', '`', '-', '?', ' ']);
    let reads_back = s.parse::<f64>().is_err() && matches!(serde_yaml_ng::from_str::<Value>(s), Ok(Value::String(ref back)) if back == s);
    if special || !reads_back {
        format!("'{}'", s.replace('\'', "''"))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_keeps_the_part_its_field_is_in() {
        let probe = "{ networks: { dmz: { cidr: 10.20.1.0/24, gateway: a } }, machines: { a: { networks: { dmz: 1 } } } }";
        assert_eq!(focus(probe, "networks.*.gateway"), "networks: { dmz: { cidr: 10.20.1.0/24, gateway: a } }");
        assert_eq!(focus("checks: [check.sh, { tcp: 'a:80' }]", "checks[]"), "checks: [check.sh, { tcp: 'a:80' }]");
        assert_eq!(focus("tools: { shell: {} }", "tools"), "tools: { shell: {} }");
        assert_eq!(focus("not: [closed", "x"), "not: [closed");
        assert_eq!(focus("machines: { a: {} }", "networks.*.cidr"), "machines: { a: {} }");
        let two = "machines: { a: { vm: null }, b: { networks: { lan: 11 }, depends_on: [a] } }";
        assert_eq!(focus(two, "machines.*.depends_on"), "machines: { b: { depends_on: [a] } }");
        assert_eq!(
            focus(two, "machines.*.count"),
            "machines: { a: { vm: null }, b: { networks: { lan: 11 }, depends_on: [a] } }"
        );
        assert_eq!(
            focus(two, "machines"),
            "machines: { a: { vm: null }, b: { networks: { lan: 11 }, depends_on: [a] } }"
        );
    }

    #[test]
    fn strings_are_quoted_only_when_yaml_would_misread_them() {
        let v: Value =
            serde_yaml_ng::from_str("{ mode: '0600', cidr: 'fd00:20::/64', cmd: [nginx, -g, 'daemon off;'], on: true, n: 2, x: !t y, e: '' }").unwrap();
        assert_eq!(
            flow(&v),
            "{ mode: '0600', cidr: 'fd00:20::/64', cmd: [nginx, '-g', daemon off;], on: true, n: 2, x: y, e: '' }"
        );
        assert_eq!(scalar("it's: here"), "'it''s: here'");
    }
}
