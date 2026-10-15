//! YAML text → [`Spec`], or every problem found on the way, coded and placed. The old keys are
//! reported first (with their fixes), `count:` clones and the shared fields fold in, then each
//! top-level section and each machine is read on its own, so one mistake doesn't hide the next.

use serde::de::DeserializeOwned;
use serde_yaml_ng::{Mapping, Value};

use super::problem::{Code, FieldPath, Problem, Problems, closest};
use super::spans::Spans;
use super::{Spec, old_keys};

/// Reads a spec from its YAML text: the spec, or its problems in file order.
///
/// # Errors
///
/// Every problem found, coded and placed: YAML that doesn't parse, the earlier format's keys,
/// unknown and missing fields, values of the wrong type or shape.
pub fn parse(yaml: &str) -> Result<Spec, Problems> {
    let spans = Spans::of(yaml);
    let mut doc: Value = serde_yaml_ng::from_str(yaml).map_err(|e| vec![yaml_problem(&e)])?;
    let mut problems = old_keys::scan(&doc);
    let clones = crate::clones::expand(&mut doc).unwrap_or_else(|refused| {
        problems.push(refused);
        indexmap::IndexMap::new()
    });
    if let Err(refused) = crate::groups::expand(&mut doc) {
        problems.push(refused);
    }
    let Value::Mapping(top) = doc else {
        return Err(vec![Problem::new(
            Code::InvalidValue,
            FieldPath::default(),
            "a spec is a mapping of fields (`version`, `name`, `machines`, ...)",
        )]);
    };
    let mut reader = Reader { problems: Vec::new() };
    let mut spec = reader.spec(top);
    spec.clones = clones;
    // A typed problem at or inside an old key's field repeats it: the old key's fix says it all.
    let old = problems.clone();
    problems.extend(reader.problems.into_iter().filter(|p| !old.iter().any(|o| p.path.within(&o.path))));
    if problems.is_empty() {
        spec.spans = spans;
        return Ok(spec);
    }
    let mut placed: Problems = problems.into_iter().map(|p| spans.place(p)).collect();
    placed.sort_by_key(|p| p.span.map(|s| (s.line, s.column)));
    Err(placed)
}

/// A YAML syntax error, at its line.
fn yaml_problem(e: &serde_yaml_ng::Error) -> Problem {
    let mut p = Problem::new(Code::InvalidValue, FieldPath::default(), format!("not valid YAML: {e}"));
    p.span = e.location().map(|l| super::Span {
        line: l.line(),
        column: l.column(),
    });
    p
}

/// Reads typed values out of the document, keeping every problem.
struct Reader {
    problems: Problems,
}

/// The top-level fields, in the order the schema lists them.
const FIELDS: &[&str] = &[
    "version",
    "name",
    "networks",
    "reach",
    "dns",
    "inputs",
    "secrets",
    "volumes",
    "machines",
    "common",
    "groups",
    "provision",
    "controller",
    "checks",
    "message",
    "overrides",
    "tools",
    "targets",
    "constraints",
    "cloud",
];

impl Reader {
    /// The spec's sections, each read on its own.
    fn spec(&mut self, mut top: Mapping) -> Spec {
        let mut spec = Spec::default();
        for (k, _) in &top {
            let key = k.as_str().unwrap_or_default();
            if !FIELDS.contains(&key) {
                self.problems.push(unknown(&FieldPath::default(), key, FIELDS));
            }
        }
        if !self.set(&mut top, "version", &mut spec.version) {
            self.problems
                .push(Problem::new(Code::MissingField, FieldPath::new("version"), "a spec says its format's version").fix("`version: 1`"));
        }
        if !self.set(&mut top, "name", &mut spec.name) {
            self.problems
                .push(Problem::new(Code::MissingField, FieldPath::new("name"), "a spec has a name (kebab-case)").fix("`name: my-lab`"));
        }
        self.set(&mut top, "networks", &mut spec.networks);
        self.set(&mut top, "reach", &mut spec.reach);
        self.set(&mut top, "dns", &mut spec.dns);
        self.set(&mut top, "inputs", &mut spec.inputs);
        self.set(&mut top, "secrets", &mut spec.secrets);
        self.set(&mut top, "volumes", &mut spec.volumes);
        self.set(&mut top, "common", &mut spec.common);
        self.set(&mut top, "groups", &mut spec.groups);
        self.set(&mut top, "provision", &mut spec.provision);
        self.set(&mut top, "controller", &mut spec.controller);
        self.set(&mut top, "checks", &mut spec.checks);
        self.set(&mut top, "message", &mut spec.message);
        self.set(&mut top, "overrides", &mut spec.overrides);
        self.set(&mut top, "tools", &mut spec.tools);
        self.set(&mut top, "targets", &mut spec.targets);
        self.set(&mut top, "constraints", &mut spec.constraints);
        self.set(&mut top, "cloud", &mut spec.cloud);
        if let Some(machines) = top.remove("machines") {
            self.machines(machines, &mut spec);
        }
        spec
    }

    /// Reads field `key` of `top` into `slot`; whether the field was written.
    fn set<T: DeserializeOwned>(&mut self, top: &mut Mapping, key: &str, slot: &mut T) -> bool {
        let Some(v) = top.remove(key) else { return false };
        if let Some(read) = self.read(v, &FieldPath::new(key)) {
            *slot = read;
        }
        true
    }

    /// Each machine, read on its own.
    fn machines(&mut self, machines: Value, spec: &mut Spec) {
        let at = FieldPath::new("machines");
        let Value::Mapping(machines) = machines else {
            if !machines.is_null() {
                self.problems
                    .push(Problem::new(Code::InvalidValue, at, "machines are a map of name to machine"));
            }
            return;
        };
        for (name, m) in machines {
            let Some(name) = name.as_str().map(str::to_owned) else {
                self.problems.push(Problem::new(Code::InvalidValue, at.clone(), "a machine's name is a text"));
                continue;
            };
            let m = if m.is_null() { Value::Mapping(Mapping::new()) } else { m };
            if let Some(machine) = self.read(m, &at.key(&name)) {
                spec.machines.insert(name, machine);
            }
        }
    }

    /// `value` read as a `T`, or its problem kept.
    fn read<T: DeserializeOwned>(&mut self, value: Value, at: &FieldPath) -> Option<T> {
        match serde_path_to_error::deserialize(value) {
            Ok(v) => Some(v),
            Err(e) => {
                self.problems.push(typed_problem(at, &e));
                None
            }
        }
    }
}

/// A typed read's error as a problem: the path down to the field, the code its message says.
fn typed_problem(at: &FieldPath, e: &serde_path_to_error::Error<serde_yaml_ng::Error>) -> Problem {
    let mut path = at.clone();
    for segment in e.path() {
        path = match segment {
            serde_path_to_error::Segment::Seq { index } => path.index(*index),
            serde_path_to_error::Segment::Map { key } => path.key(key),
            serde_path_to_error::Segment::Enum { variant } => path.key(variant),
            serde_path_to_error::Segment::Unknown => path,
        };
    }
    let message = e.inner().to_string();
    if let Some(rest) = message.strip_prefix("unknown field `") {
        let (field, expected) = rest.split_once('`').unwrap_or((rest, ""));
        let known: Vec<&str> = expected.split('`').skip(1).step_by(2).collect();
        // The path may already end at the unknown field.
        let parent = if path.as_str().ends_with(&format!(".{field}")) {
            path.parent().unwrap_or_default()
        } else {
            path
        };
        return unknown(&parent, field, &known);
    }
    if let Some(rest) = message.strip_prefix("missing field `") {
        let field = rest.split('`').next().unwrap_or(rest);
        return Problem::new(Code::MissingField, path.key(field), format!("`{field}` is required here"));
    }
    let code = if message.contains("both given") || message.contains("are all given") {
        Code::Conflict
    } else {
        Code::InvalidValue
    };
    Problem::new(code, path, message)
}

/// An unknown field, with the closest known one as the fix.
fn unknown(at: &FieldPath, field: &str, known: &[&str]) -> Problem {
    let p = Problem::new(Code::UnknownField, at.key(field), format!("`{field}` isn't a field here"));
    match closest(field, known) {
        Some(k) => p.fix(format!("did you mean `{k}`?")),
        None if known.is_empty() => p,
        None => p.fix(format!(
            "the fields here: {}",
            known.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")
        )),
    }
}

#[cfg(test)]
mod tests;
