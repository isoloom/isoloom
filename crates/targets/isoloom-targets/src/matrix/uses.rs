//! Whether a spec, as its author wrote it, uses a row: the row's field written in it, with its
//! value or its count of entries, beside the fields the row goes with on the same machine,
//! network or item. A field `common:` or a group gives every machine counts as the machines'.

use serde_yaml_ng::Value;

use super::probe::Written;

/// A step of a field's path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Segment<'a> {
    /// A key of a mapping.
    Key(&'a str),
    /// Any key of a mapping (`*`).
    AnyKey,
    /// Any item of a list (`[]`).
    AnyItem,
}

/// The steps of `path` (`machines.*.files[].secret`).
fn segments(path: &str) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    for part in path.split('.') {
        let (key, items) = part.strip_suffix("[]").map_or((part, false), |k| (k, true));
        match key {
            "*" => out.push(Segment::AnyKey),
            "" => {}
            k => out.push(Segment::Key(k)),
        }
        if items {
            out.push(Segment::AnyItem);
        }
    }
    out
}

/// The values at `path` under `node`.
fn reach<'a>(node: &'a Value, path: &[Segment<'_>]) -> Vec<&'a Value> {
    let Some((first, rest)) = path.split_first() else {
        return vec![node];
    };
    let next: Vec<&Value> = match first {
        Segment::Key(k) => node.get(*k).into_iter().collect(),
        Segment::AnyKey => node.as_mapping().map(|m| m.values().collect()).unwrap_or_default(),
        Segment::AnyItem => node.as_sequence().map(|s| s.iter().collect()).unwrap_or_default(),
    };
    next.into_iter().flat_map(|n| reach(n, rest)).collect()
}

/// What the row's conditions are checked on: the values at the path up to its first `*` or `[]`
/// (each machine, network or item), or the whole spec for a path with neither; with the rest of
/// the path. The machines' are joined by `common:` and each group, which give machines fields.
fn subjects<'a, 'p>(spec: &'a Value, path: &'p [Segment<'p>]) -> (Vec<&'a Value>, &'p [Segment<'p>]) {
    let Some(cut) = path.iter().position(|s| !matches!(s, Segment::Key(_))) else {
        return (vec![spec], path);
    };
    let (head, rest) = path.split_at(cut + 1);
    let mut found = reach(spec, head);
    if head == [Segment::Key("machines"), Segment::AnyKey] {
        found.extend(spec.get("common"));
        found.extend(spec.get("groups").and_then(Value::as_mapping).into_iter().flat_map(|g| g.values()));
    }
    (found, rest)
}

/// A scalar as text: a string, a number or a boolean.
fn text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Whether `text` matches `pattern`, where one `*` stands for any text.
fn matches(pattern: &str, text: &str) -> bool {
    match pattern.split_once('*') {
        Some((head, tail)) => text.len() >= head.len() + tail.len() && text.starts_with(head) && text.ends_with(tail),
        None => pattern == text,
    }
}

/// How many entries a mapping or a list holds; a scalar holds none.
fn entries(v: &Value) -> usize {
    match v {
        Value::Mapping(m) => m.len(),
        Value::Sequence(s) => s.len(),
        _ => 0,
    }
}

/// Whether the field `path` as written in a spec (`machines.web.app.steps[0]`) is the field
/// `pattern` names (`machines.*.app.steps[]`).
pub(super) fn path_matches(pattern: &str, path: &str) -> bool {
    let concrete = segments_of(path);
    let wanted = segments(pattern);
    concrete.len() == wanted.len()
        && wanted.iter().zip(&concrete).all(|(w, c)| match (w, c) {
            (Segment::AnyKey, Segment::Key(_)) | (Segment::AnyItem, Segment::AnyItem) => true,
            (w, c) => w == c,
        })
}

/// The steps of a path as written, each `[n]` an item.
fn segments_of(path: &str) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    for part in path.split('.') {
        let (key, items) = part.split_once('[').map_or((part, 0), |(k, rest)| (k, rest.matches('[').count() + 1));
        if !key.is_empty() {
            out.push(Segment::Key(key));
        }
        out.extend(std::iter::repeat_n(Segment::AnyItem, items));
    }
    out
}

/// Whether `subject` has `field` (a path under it), or, written `path=pattern`, that field with a
/// value matching the pattern.
fn has(subject: &Value, field: &str) -> bool {
    let (path, pattern) = field.split_once('=').map_or((field, None), |(p, v)| (p, Some(v)));
    let leaves = reach(subject, &segments(path));
    match pattern {
        None => !leaves.is_empty(),
        Some(p) => leaves.iter().filter_map(|l| text(l)).any(|t| matches(p, &t)),
    }
}

/// Whether `spec` (the YAML as written) has the field `w` describes, on one machine, network or
/// item that also meets its other conditions.
pub(super) fn written_in(w: &Written, spec: &Value) -> bool {
    let path = segments(&w.field);
    let (found, rest) = subjects(spec, &path);
    found.into_iter().any(|subject| {
        let leaves = reach(subject, rest);
        let valued = w.values.is_empty() || leaves.iter().filter_map(|l| text(l)).any(|t| w.values.iter().any(|p| matches(p, &t)));
        let counted = w.count.is_none_or(|n| leaves.iter().any(|l| entries(l) >= n));
        let has = |field: &String| has(subject, field);
        !leaves.is_empty() && valued && counted && w.also.iter().all(has) && !w.without.iter().any(has)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(field: &str) -> Written {
        Written {
            field: field.to_owned(),
            values: Vec::new(),
            count: None,
            also: Vec::new(),
            without: Vec::new(),
        }
    }

    fn spec(text: &str) -> Value {
        serde_yaml_ng::from_str(text).unwrap()
    }

    const LAB: &str = "
networks: { lan: { cidr: 10.0.0.0/24, ipv6: { cidr: 'fd00::/64' } }, wan: { cidr: 10.0.1.0/24 } }
common: { hardware: { arch: arm64 } }
machines:
  web: { os: debian@12, networks: { lan: [10, 11] }, ports: { web: 80 }, app: { steps: [{ run: a.sh }, reboot] } }
  db: { os: windows-server@2022, networks: { lan: 20, wan: 20 }, command: x }
checks: [checks/a.sh, { from: web, exec: 'true' }]
";

    #[test]
    fn a_path_as_written_is_the_field_its_pattern_names() {
        assert!(path_matches("machines.*.app.image", "machines.web.app.image"));
        assert!(path_matches("machines.*.app.steps[]", "machines.web.app.steps[3]"));
        assert!(path_matches("overrides.external.machines.*", "overrides.external.machines.db"));
        assert!(!path_matches("machines.*.app.image", "machines.web.app.build"));
        assert!(!path_matches("machines.*.app", "machines.web.app.image"));
    }

    #[test]
    fn a_field_is_found_under_any_name_and_any_item() {
        let lab = spec(LAB);
        assert!(written_in(&written("networks.*.ipv6"), &lab));
        assert!(written_in(&written("checks[].exec"), &lab));
        assert!(!written_in(&written("networks.*.mtu"), &lab));
        assert!(written_in(&written("machines.*.hardware.arch"), &lab), "from `common:`");
    }

    #[test]
    fn a_value_a_count_and_the_fields_beside_narrow_it_to_one_machine() {
        let lab = spec(LAB);
        let valued = |field: &str, value: &str| Written {
            values: vec![value.to_owned()],
            ..written(field)
        };
        assert!(written_in(&valued("machines.*.os", "windows-server@*"), &lab));
        assert!(!written_in(&valued("machines.*.os", "windows@*"), &lab));
        assert!(written_in(&valued("machines.*.app.steps[]", "reboot"), &lab));
        assert!(written_in(&valued("checks[]", "*.sh"), &lab) && !written_in(&valued("checks[]", "*.yml"), &lab));
        let counted = |field: &str| Written {
            count: Some(2),
            ..written(field)
        };
        assert!(written_in(&counted("machines"), &lab) && written_in(&counted("machines.*.networks"), &lab));
        assert!(written_in(&counted("machines.*.networks.*"), &lab));
        let serving = Written {
            also: vec!["ports".to_owned()],
            without: vec!["command".to_owned()],
            ..written("machines.*.app.steps")
        };
        assert!(written_in(&serving, &lab), "web has steps and ports, and no command");
        let commanded = Written {
            without: vec!["ports".to_owned()],
            ..written("machines.*.command")
        };
        assert!(written_in(&commanded, &lab), "db has a command and no ports");
        assert!(
            !written_in(
                &Written {
                    also: vec!["ports".to_owned()],
                    ..commanded.clone()
                },
                &lab
            ),
            "on the same machine"
        );
        let windows = |pattern: &str| Written {
            also: vec![format!("os={pattern}")],
            ..written("machines.*.command")
        };
        assert!(written_in(&windows("windows-server@*"), &lab) && !written_in(&windows("debian@*"), &lab));
    }
}
