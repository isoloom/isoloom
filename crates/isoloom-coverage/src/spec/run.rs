//! Running a field's probe through an output's backend: whether it renders, refuses (and why),
//! or can't be asked (the probe doesn't suit the target, or isn't a valid spec).

use isoloom_spec::model::{Spec, Target};
use isoloom_targets::{Environment, GenerateError, RenderError};
use serde_yaml_ng::{Mapping, Value};

use super::output::Output;
use super::probes::{BASELINE, CLOUD_BASELINE, cloud_probe, probe};

/// What an output's backend makes of a probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The backend renders it.
    Renders,
    /// The backend refuses it, with its reason.
    Refused(String),
    /// The target can't run the probe at all (no machine of the kind it needs).
    NotPossible,
    /// The field has no probe, or its probe isn't a valid spec (the tests reject both).
    NoProbe(String),
}

/// A field's probe documents for an output: the cloud-services baseline for cloud services,
/// else the machines' one. Hybrid gets two, so the field is tried on a container and on a VM.
pub fn documents(path: &str, output: Output) -> Result<Vec<Value>, String> {
    let (base, fragment) = if output == Output::CloudServices {
        (CLOUD_BASELINE, cloud_probe(path))
    } else {
        (BASELINE, probe(path))
    };
    let mut doc = yaml(base)?;
    match fragment {
        Some(f) => merge(&mut doc, yaml(&f)?),
        None if super::paths(&doc).iter().any(|p| p == path) => {}
        None => return Err(format!("`{path}` has no probe for {}", output.label())),
    }
    if output != Output::Hybrid {
        return Ok(vec![doc]);
    }
    let mut vm_first = doc.clone();
    add_machine(&mut doc, "v", "{ networks: { lan: 12 }, vm: { os: debian-12, provision: [setup.sh] } }")?;
    if let Some(Value::Mapping(a)) = vm_first.get_mut("machines").and_then(|m| m.get_mut("a")) {
        a.remove("docker");
    }
    add_machine(&mut vm_first, "c", "{ networks: { lan: 13 }, docker: { image: nginx } }")?;
    Ok(vec![doc, vm_first])
}

/// What `output` makes of the field's probe: refused if any of its documents is refused.
pub fn outcome(path: &str, output: Output) -> Outcome {
    let docs = match documents(path, output) {
        Ok(d) => d,
        Err(e) => return Outcome::NoProbe(e),
    };
    let mut result = Outcome::NotPossible;
    for doc in docs {
        match try_document(&doc, output) {
            r @ (Outcome::Refused(_) | Outcome::NoProbe(_)) => return r,
            Outcome::Renders => result = Outcome::Renders,
            Outcome::NotPossible => {}
        }
    }
    result
}

/// One document through the output's backend.
fn try_document(doc: &Value, output: Output) -> Outcome {
    let spec = match serde_yaml_ng::to_string(doc)
        .map_err(|e| e.to_string())
        .and_then(|t| isoloom_spec::parse(&t).map_err(|e| e.to_string()))
    {
        Ok(s) => s,
        Err(e) => return Outcome::NoProbe(e),
    };
    if let Some(problem) = isoloom_spec::validate(&spec).first() {
        return Outcome::NoProbe(format!("invalid probe: {problem}"));
    }
    match render(&spec, output) {
        None => Outcome::NotPossible,
        Some(Ok(())) => Outcome::Renders,
        Some(Err(why)) => Outcome::Refused(why),
    }
}

/// The output's backend rendering `spec`, when its target can run it.
pub fn render(spec: &Spec, output: Output) -> Option<Result<(), String>> {
    let target = output.target();
    if !isoloom_spec::effective(spec).contains(&target) {
        return None;
    }
    let env = Environment::build(spec, target, None);
    let result = match output {
        Output::CloudVm(cloud) => {
            let (_, backend) = isoloom_terraform::cloud_vm::CLOUDS.iter().find(|(c, _)| *c == cloud)?;
            backend.render(&env)
        }
        _ => isoloom_targets::backend(target)?.render(&env),
    };
    Some(result.map(|_| ()).map_err(|e| reason(e, target)))
}

/// A refusal in the backend's words, without the target's name in front.
fn reason(e: RenderError, target: Target) -> String {
    match e {
        RenderError::Refused { refusal, .. } => refusal.to_string(),
        RenderError::Generate(GenerateError::Unsupported { what, .. }) => what,
        RenderError::Generate(other) => {
            let text = other.to_string();
            text.strip_prefix(&format!("{}: ", target.id())).map_or(text.clone(), str::to_string)
        }
    }
}

fn yaml(text: &str) -> Result<Value, String> {
    serde_yaml_ng::from_str(text).map_err(|e| format!("probe YAML: {e}"))
}

/// `from` merged into `into`: maps merge key by key, `null` removes a key, anything else replaces.
fn merge(into: &mut Value, from: Value) {
    match (into, from) {
        (Value::Mapping(into), Value::Mapping(from)) => {
            for (k, v) in from {
                if v.is_null() {
                    into.remove(&k);
                } else if let Some(slot) = into.get_mut(&k) {
                    merge(slot, v);
                } else {
                    into.insert(k, v);
                }
            }
        }
        (into, from) => *into = from,
    }
}

fn add_machine(doc: &mut Value, name: &str, machine: &str) -> Result<(), String> {
    let mut extra = Mapping::new();
    extra.insert(Value::from(name), yaml(machine)?);
    let mut top = Mapping::new();
    top.insert(Value::from("machines"), Value::Mapping(extra));
    merge(doc, Value::Mapping(top));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merging_replaces_values_and_null_removes_a_key() {
        let mut doc = yaml("{ a: { b: 1, c: [1] }, d: 2 }").unwrap();
        merge(&mut doc, yaml("{ a: { b: null, c: [2] }, e: 3 }").unwrap());
        assert_eq!(doc, yaml("{ a: { c: [2] }, d: 2, e: 3 }").unwrap());
    }

    #[test]
    fn a_field_without_a_probe_or_an_invalid_probe_is_no_probe() {
        assert!(matches!(outcome("no.such.field", Output::Docker), Outcome::NoProbe(_)));
        assert!(matches!(try_document(&yaml("{ version: 1 }").unwrap(), Output::Docker), Outcome::NoProbe(_)));
        assert!(yaml("{ a: ").is_err());
    }

    #[test]
    fn a_target_that_cannot_run_the_probe_is_not_asked() {
        assert_eq!(
            outcome("cloud.vars", Output::Docker),
            Outcome::NoProbe("`cloud.vars` has no probe for docker".to_string())
        );
        assert_eq!(outcome("machines.*.docker.appliance", Output::Vagrant), Outcome::NotPossible);
        assert_eq!(outcome("machines.*.ulimits", Output::Kubernetes), outcome("machines.*.ulimits", Output::Helm));
    }

    #[test]
    fn a_refusal_reads_without_its_target() {
        let e = RenderError::Generate(GenerateError::NotPossible(Target::Docker));
        assert!(!reason(e, Target::Docker).starts_with("docker: "));
        let e = RenderError::Generate(GenerateError::Unsupported {
            target: Target::Docker,
            what: "no".to_string(),
        });
        assert_eq!(reason(e, Target::Docker), "no");
    }
}
