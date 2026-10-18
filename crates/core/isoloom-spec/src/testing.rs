//! What the unit tests share: a lowered spec ([`crate::model::Spec`]) read from YAML written in
//! its own shape (`docker:` and `vm:` blocks), the way the plan lowers an authored one. Clones,
//! `common:` and groups fold in, a machine's several addresses split, each OS resolves and VLANs
//! become networks, so a test states the lowered spec it checks in a few lines.

use serde_yaml_ng::{Mapping, Value};

use crate::model::Spec;
use crate::{clones, groups, os, vlans};

/// A spec named `t` with one network, `lan` (10.0.0.0/24), then `rest` (its machines, checks,
/// volumes...), as YAML.
pub(crate) fn lab(rest: &str) -> String {
    format!("version: 1\nname: t\nnetworks:\n  lan: {{ cidr: 10.0.0.0/24 }}\n{rest}")
}

/// A fresh, empty folder for test `name` under the system's temporary folder.
pub(crate) fn folder(name: &str) -> std::io::Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join(format!("isoloom-spec-{}-{name}", std::process::id()));
    match std::fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    std::fs::create_dir_all(&dir).map(|()| dir)
}

/// What validation finds in the lowered spec `yaml` describes, each as `path: message`.
pub(crate) fn problems(yaml: &str) -> Vec<String> {
    match model(yaml) {
        Ok(spec) => crate::validate(&spec).iter().map(ToString::to_string).collect(),
        Err(why) => vec![format!("doesn't read: {why}")],
    }
}

/// The lowered spec `yaml` describes, or why it doesn't read.
pub(crate) fn model(yaml: &str) -> Result<Spec, String> {
    let mut doc: Value = serde_yaml_ng::from_str(yaml).map_err(|e| e.to_string())?;
    // A machine's `os` reaches its `vm:` before clones copy the machine and before a shared
    // `vm:` could fill its OS.
    fill_vm(&mut doc);
    let clones = clones::expand(&mut doc).map_err(|p| p.line())?;
    let own = implementations(&doc);
    groups::expand(&mut doc).map_err(|p| p.line())?;
    complete_only(&mut doc, &own);
    split_addresses(&mut doc)?;
    let mut spec: Spec = serde_yaml_ng::from_value(doc).map_err(|e| e.to_string())?;
    spec.clones = clones;
    resolve_os(&mut spec)?;
    vlans::flatten(spec).map_err(|p| p.line())
}

/// Gives each machine's `os` to its `vm:`: a machine with neither `docker:` nor `vm:` gets
/// `vm: { os }`, and a `vm:` without `os` gets the machine's.
fn fill_vm(doc: &mut Value) {
    let Some(Value::Mapping(machines)) = doc.get_mut("machines") else { return };
    for m in machines.values_mut() {
        let Value::Mapping(m) = m else { continue };
        let Some(os) = m.get("os").filter(|v| v.is_string()).cloned() else { continue };
        if !m.contains_key("docker") && !m.contains_key("vm") {
            m.insert("vm".into(), Value::Mapping(Mapping::new()));
        }
        if let Some(Value::Mapping(vm)) = m.get_mut("vm")
            && !vm.contains_key("os")
        {
            vm.insert("os".into(), os);
        }
    }
}

/// Which of `docker:` and `vm:` each machine declares itself.
fn implementations(doc: &Value) -> Vec<(String, [bool; 2])> {
    let Some(Value::Mapping(machines)) = doc.get("machines") else {
        return Vec::new();
    };
    machines
        .iter()
        .filter_map(|(name, m)| Some((name.as_str()?.to_owned(), [m.get("docker").is_some(), m.get("vm").is_some()])))
        .collect()
}

/// A shared `docker:` or `vm:` only completes one a machine declares itself, never adds one.
fn complete_only(doc: &mut Value, own: &[(String, [bool; 2])]) {
    let Some(Value::Mapping(machines)) = doc.get_mut("machines") else { return };
    for (name, [docker, vm]) in own {
        if let Some(Value::Mapping(m)) = machines.get_mut(name.as_str()) {
            for (key, declared) in [("docker", docker), ("vm", vm)] {
                if !declared {
                    m.remove(key);
                }
            }
        }
    }
}

/// Splits every machine's address lists: the first stays in `networks`, the others go to
/// `more_addresses`.
fn split_addresses(doc: &mut Value) -> Result<(), String> {
    let Some(Value::Mapping(machines)) = doc.get_mut("machines") else {
        return Ok(());
    };
    for (name, m) in machines.iter_mut() {
        let (Some(name), Value::Mapping(m)) = (name.as_str(), m) else { continue };
        if m.contains_key("more_addresses") {
            return Err(format!("machines.{name}.more_addresses: not a field of the spec"));
        }
        let Some(Value::Mapping(nets)) = m.get_mut("networks") else { continue };
        let mut more = Mapping::new();
        for (net, value) in nets.iter_mut() {
            let Value::Sequence(list) = value else { continue };
            let at = format!("machines.{name}.networks.{}", net.as_str().unwrap_or_default());
            if list.iter().any(|o| o.as_u64().is_none()) {
                return Err(format!("{at}: each address is a number"));
            }
            let Some((first, others)) = list.split_first() else {
                return Err(format!("{at}: an empty list"));
            };
            if !others.is_empty() {
                more.insert(net.clone(), Value::Sequence(others.to_vec()));
            }
            *value = first.clone();
        }
        if !more.is_empty() {
            m.insert("more_addresses".into(), Value::Mapping(more));
        }
    }
    Ok(())
}

/// Writes each machine's OS resolved: its `os` as `<family>@<version>`, its `vm.os` as the
/// image-table name every target reads.
fn resolve_os(spec: &mut Spec) -> Result<(), String> {
    for (name, m) in &mut spec.machines {
        if let Some(text) = &m.os {
            m.os = Some(os::resolve(text).map_err(|e| format!("machines.{name}.os: {e}"))?.id());
        }
        if let Some(vm) = m.vm.as_mut().filter(|vm| !vm.os.is_empty()) {
            os::resolve(&vm.os)
                .map_err(|e| format!("machines.{name}.vm.os: {e}"))?
                .name
                .clone_into(&mut vm.os);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
