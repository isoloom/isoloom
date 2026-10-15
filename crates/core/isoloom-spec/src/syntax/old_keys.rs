//! The earlier format's keys, recognised on the raw YAML before the typed parse, each with the
//! targeted fix: what to write now. Nothing here is read as the new form (there is no alias),
//! so this isn't compatibility; it only keeps an author from being stranded. The module goes
//! once the CTF labs are rewritten (Phase 7).

use serde_yaml_ng::{Mapping, Value};

use super::app::has_extension;
use super::problem::{Code, FieldPath, Problem};

/// The old keys of a spec document, each a `removed-field` problem with its fix (a list of
/// ports is an `invalid-value`: the field stays, its shape changed).
pub(super) fn scan(doc: &Value) -> Vec<Problem> {
    let mut out = Vec::new();
    let Value::Mapping(top) = doc else { return out };
    if let Some(Value::Mapping(machines)) = top.get("machines") {
        for (name, m) in machines {
            if let (Some(name), Value::Mapping(m)) = (name.as_str(), m) {
                machine(&FieldPath::new("machines").key(name), name, m, &mut out);
            }
        }
    }
    if let Some(Value::Mapping(m)) = top.get("common") {
        shared(&FieldPath::new("common"), m, &mut out);
    }
    if let Some(Value::Mapping(groups)) = top.get("groups") {
        for (name, g) in groups {
            if let (Some(name), Value::Mapping(g)) = (name.as_str(), g) {
                shared(&FieldPath::new("groups").key(name), g, &mut out);
            }
        }
    }
    networks(top, &mut out);
    volumes(top, &mut out);
    if let Some(Value::Mapping(c)) = top.get("controller")
        && c.contains_key("image")
    {
        let fix = "`overrides: { vagrant: { controller: { box: <box>, box_version: <version> } } }`";
        out.push(removed(FieldPath::new("controller.image"), "the controller's box is Vagrant's own fact").fix(fix));
    }
    out
}

fn removed(path: FieldPath, reason: &str) -> Problem {
    Problem::new(Code::RemovedField, path, reason.to_string())
}

/// A machine's old keys.
fn machine(at: &FieldPath, name: &str, m: &Mapping, out: &mut Vec<Problem>) {
    if let Some(Value::Mapping(d)) = m.get("docker") {
        let reason = "a machine says what it runs, never how a platform runs it, so `docker:` is gone";
        out.push(removed(at.key("docker"), reason).fix(docker_fix(name, d)));
    }
    if let Some(Value::Mapping(v)) = m.get("vm") {
        let reason = "a machine says what it runs, never how a platform runs it, so `vm:` is gone";
        out.push(removed(at.key("vm"), reason).fix(vm_fix(name, v)));
    }
    if m.contains_key("external") {
        let fix = format!("`overrides: {{ external: {{ machines: {{ {name}: {{ ssh: {{ address, user, port, key }} }} }} }} }}`");
        out.push(removed(at.key("external"), "an existing host is a fact of the `external` target").fix(fix));
    }
    if m.contains_key("services") {
        out.push(removed(at.key("services"), "ports are `ports:`, keyed by name").fix("`ports: { web: { port: 80, protocol: http, publish: 8080 } }`"));
    }
    if let Some(Value::Sequence(list)) = m.get("ports") {
        let p = Problem::new(Code::InvalidValue, at.key("ports"), "ports are a map of named ports, not a list");
        out.push(p.fix(format!("`ports: {{ {} }}`", ports_fix(list))));
    }
    shared(at, m, out);
}

/// The old keys a machine, `common:` or a group may hold.
fn shared(at: &FieldPath, m: &Mapping, out: &mut Vec<Problem>) {
    let moved: [(&str, &str, String); 8] = [
        (
            "inputs",
            "an input reaches a machine as a file or an environment value",
            "`files: [{ path: /ctf/flag, input: <NAME> }]`, or `environment: { <NAME>: { input: <NAME> } }` for a non-sensitive input".to_string(),
        ),
        (
            "secrets",
            "a secret reaches a machine as a file",
            "`files: [{ path: /run/secrets/<name>, secret: <name> }]`".to_string(),
        ),
        ("arch", "the architecture is hardware", "`hardware: { arch: <arch> }`".to_string()),
        ("gpus", "GPUs are hardware", "`hardware: { gpus: <gpus> }`".to_string()),
        ("devices", "host devices are hardware", "`hardware: { devices: [...] }`".to_string()),
        (
            "nested_virtualization",
            "nested virtualization is hardware",
            "`hardware: { nested_virtualization: true }`".to_string(),
        ),
        (
            "before",
            "a per-run value lands as a file before the process starts, and set-up runs once the machine is up",
            "`files: [{ path, input }]`, or `setup: [...]`".to_string(),
        ),
        (
            "working_dir",
            "the directory belongs to the command",
            "`command: { run: <command>, cwd: <dir> }`".to_string(),
        ),
    ];
    for (key, reason, fix) in moved {
        if m.contains_key(key) {
            out.push(removed(at.key(key), reason).fix(fix));
        }
    }
    if !at.as_str().starts_with("machines.") {
        for key in ["docker", "vm"] {
            if m.contains_key(key) {
                let reason = format!("a machine says what it runs, never how a platform runs it, so `{key}:` is gone");
                out.push(removed(at.key(key), &reason).fix("`os: <family>@<version>` and `app: { image | build | steps }`"));
            }
        }
    }
}

/// What a machine's `docker:` block becomes.
fn docker_fix(name: &str, d: &Mapping) -> String {
    let text = |k: &str| d.get(k).and_then(Value::as_str);
    let mut parts = Vec::new();
    if let Some(a) = text("appliance") {
        parts.push(format!("`app: {{ appliance: {a}, image | firmware, config }}`"));
    } else if d.get("idle").and_then(Value::as_bool) == Some(true) {
        parts.push("`os: <family>@<version>` alone: the OS's own image, kept running idle".to_string());
    } else if let Some(b) = text("build") {
        if d.contains_key("dockerfile") || d.contains_key("args") || d.contains_key("target") {
            parts.push(format!("`app: {{ build: {{ context: {b}, dockerfile, target, args }} }}`"));
        } else {
            parts.push(format!("`app: {{ build: {b} }}`"));
        }
    } else if let Some(i) = text("image") {
        parts.push(format!("`app: {{ image: {i} }}`"));
    }
    if d.contains_key("init") {
        parts.push("`init` → `setup: [...]`, and a flag script becomes `files: [{ path, input }]`".to_string());
    }
    if d.get("lxc").and_then(Value::as_bool) == Some(true) {
        parts.push(format!("`overrides: {{ proxmox: {{ machines: {{ {name}: {{ lxc: true }} }} }} }}`"));
    }
    if parts.is_empty() {
        parts.push("`app: { build: <folder> }` for your Dockerfile, or `app: { image: <ref> }`".to_string());
    }
    parts.join("; ")
}

/// What a machine's `vm:` block becomes.
fn vm_fix(name: &str, v: &Mapping) -> String {
    let mut parts = Vec::new();
    if let Some(os) = v.get("os").and_then(Value::as_str) {
        parts.push(format!("`os: {os}`"));
    }
    if let Some(Value::Sequence(steps)) = v.get("provision") {
        let steps: Vec<String> = steps.iter().filter_map(Value::as_str).map(step_fix).collect();
        parts.push(format!("`app: {{ steps: [{}] }}`", steps.join(", ")));
    }
    if v.contains_key("image") {
        parts.push(format!(
            "`vm.image` → `overrides: {{ vagrant: {{ machines: {{ {name}: {{ box, box_version }} }} }} }}` (and `proxmox`, the clouds)"
        ));
    }
    if let Some(tpm) = v.get("tpm").and_then(Value::as_bool) {
        parts.push(format!("`hardware: {{ tpm: {} }}`", if tpm { "required" } else { "none" }));
    }
    if v.contains_key("boot") {
        parts.push("`hardware: { boot: ... }`".to_string());
    }
    if parts.is_empty() {
        parts.push("`os: <family>@<version>`".to_string());
    }
    parts.join("; ")
}

/// A `vm.provision` step as an `app.steps` item.
fn step_fix(s: &str) -> String {
    if s == "reboot" {
        "reboot".to_string()
    } else if has_extension(s, "yml") || has_extension(s, "yaml") {
        format!("{{ playbook: {s} }}")
    } else {
        format!("{{ run: {s} }}")
    }
}

/// A list of ports written as the map that replaces it.
fn ports_fix(list: &[Value]) -> String {
    let items: Vec<String> = list
        .iter()
        .filter_map(Value::as_mapping)
        .map(|p| {
            let number = p.get("port").and_then(Value::as_u64).unwrap_or_default();
            let flag = |k: &str| p.get(k).and_then(Value::as_bool) == Some(true);
            let protocol = match (p.get("protocol").and_then(Value::as_str), flag("http"), flag("tls")) {
                (Some("udp"), ..) => Some("udp"),
                (_, true, true) => Some("https"),
                (_, true, false) => Some("http"),
                (_, false, true) => Some("tls"),
                _ => None,
            };
            let name = p.get("name").and_then(Value::as_str).map_or_else(|| format!("p{number}"), str::to_string);
            let mut fields = vec![format!("port: {number}")];
            fields.extend(protocol.map(|p| format!("protocol: {p}")));
            if let Some(h) = p.get("publish").and_then(Value::as_u64) {
                fields.push(if flag("fixed") {
                    format!("publish: {{ host: {h}, fixed: true }}")
                } else {
                    format!("publish: {h}")
                });
            }
            if flag("dormant") {
                fields.push("dormant: true".to_string());
            }
            format!("{name}: {{ {} }}", fields.join(", "))
        })
        .collect();
    items.join(", ")
}

/// A network's and a volume's platform names, moved to `overrides:`.
fn networks(top: &Mapping, out: &mut Vec<Problem>) {
    let Some(Value::Mapping(nets)) = top.get("networks") else { return };
    for (n, net) in nets {
        let (Some(n), Value::Mapping(net)) = (n.as_str(), net) else { continue };
        let at = FieldPath::new("networks").key(n);
        if let Some(Value::Mapping(d)) = net.get("docker") {
            let cidr = d.get("cidr").and_then(Value::as_str).unwrap_or("<block>");
            let fix = format!("`overrides: {{ docker: {{ networks: {{ {n}: {{ cidr: {cidr} }} }} }} }}`");
            out.push(removed(at.key("docker"), "a network's Docker block is a fact of the Docker targets").fix(fix));
        }
        if let Some(name) = net.get("external").and_then(Value::as_str) {
            let fix = format!("`overrides: {{ <target>: {{ networks: {{ {n}: {{ external: {name} }} }} }} }}`, for `docker`, `proxmox` or `esxi`");
            out.push(removed(at.key("external"), "an existing network has a name on one target").fix(fix));
        }
    }
}

fn volumes(top: &Mapping, out: &mut Vec<Problem>) {
    let Some(Value::Mapping(vols)) = top.get("volumes") else { return };
    for (v, vol) in vols {
        let (Some(v), Value::Mapping(vol)) = (v.as_str(), vol) else { continue };
        let at = FieldPath::new("volumes").key(v);
        if let Some(name) = vol.get("external").and_then(Value::as_str) {
            let fix = format!("`overrides: {{ docker: {{ volumes: {{ {v}: {{ external: {name} }} }} }} }}` (or `kubernetes`)");
            out.push(removed(at.key("external"), "an existing volume has a name on one platform").fix(fix));
        }
        if vol.contains_key("virtiofs") {
            let fix = format!("`overrides: {{ proxmox: {{ volumes: {{ {v}: {{ virtiofs: {{}} }} }} }} }}`");
            out.push(removed(at.key("virtiofs"), "virtiofs is Proxmox's way to share a volume").fix(fix));
        }
    }
}

#[cfg(test)]
mod tests;
