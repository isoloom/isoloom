//! A service's keys, one conversion each: what the machine gets from it, and the notes on what
//! it doesn't.

use serde_yaml_ng::{Mapping, Value};

use super::super::{LEFT_OUT, NoteKind, label};
use super::values::{
    VolumeMount, container_ports, cpus, env_entries, flat, input_name, memory_mb, other_protocols, published_ports, str_of, variables, volume_mount,
};
use super::{Import, Machine, Service};

impl Import<'_> {
    /// One key of a service (networks were read before).
    pub(super) fn key(&mut self, svc: &Service, k: &str, v: &Value, m: &mut Machine) {
        let kat = format!("{}.{k}", svc.at);
        match k {
            "networks" | "network_mode" => {}
            "image" if svc.keys.contains_key("build") => {}
            "image" => self.image(kat, v, m),
            "build" => self.build(svc, &kat, v, m),
            "ports" | "expose" => self.ports(&kat, k == "ports", v, m),
            "environment" => self.environment(kat, v, m),
            "depends_on" => {
                let deps: Vec<String> = match v {
                    Value::Sequence(list) => list.iter().filter_map(str_of).collect(),
                    Value::Mapping(map) => map.keys().filter_map(str_of).collect(),
                    _ => Vec::new(),
                };
                m.depends_on.extend(deps.iter().filter_map(|d| self.names.get(d).cloned()));
            }
            "cpus" => m.cpus = cpus(v),
            "mem_limit" => m.memory_mb = str_of(v).and_then(|s| memory_mb(&s)),
            "deploy" => self.deploy(kat, v, m),
            "command" | "entrypoint" => self
                .notes
                .add(kat, NoteKind::InImage, format!("set it in the image (CMD / ENTRYPOINT): {}", flat(v))),
            "volumes" => self.volumes(kat, v, m),
            "hostname" | "container_name" => self
                .notes
                .add(kat, NoteKind::Equivalent, format!("the hostname is the machine's name, `{}`", m.name)),
            "healthcheck" => self.notes.add(
                kat,
                NoteKind::Equivalent,
                "Isoloom probes every service port; a custom check belongs in `checks`",
            ),
            "env_file" => self.notes.add(
                kat,
                NoteKind::Equivalent,
                "values given at launch are `inputs`: list the ones the machine needs",
            ),
            _ if k.starts_with("x-") => self.notes.add(kat, NoteKind::Tooling, "extension field, ignored"),
            _ => self.notes.add(kat, NoteKind::Changed, LEFT_OUT),
        }
    }

    fn image(&mut self, kat: String, v: &Value, m: &mut Machine) {
        let image = str_of(v).unwrap_or_default();
        if image.contains('$') {
            self.notes.add(
                kat,
                NoteKind::Changed,
                format!("`{image}` reads shell variables; kept as written, fix it to one image"),
            );
        }
        m.image = Some(image);
    }

    fn build(&mut self, svc: &Service, kat: &str, v: &Value, m: &mut Machine) {
        let (context, extra) = build_context(v);
        if context.starts_with("..") || context.starts_with('/') || context.contains("://") {
            self.notes
                .add(kat, NoteKind::Changed, format!("context `{context}` is outside the project; copy it in"));
        }
        if !extra.is_empty() {
            self.notes.add(
                kat,
                NoteKind::InImage,
                format!("only the context is kept, not: {} (build settings belong in the Dockerfile)", extra.join(", ")),
            );
        }
        if let Value::Mapping(b) = v {
            if let Some(f) = dockerfile(&context, b) {
                m.dockerfile = Some(f);
            }
            m.target = b.get("target").and_then(str_of);
            self.build_args(svc, b, m);
        }
        m.build = Some(context);
        if svc.keys.contains_key("image") {
            self.notes
                .add(format!("{}.image", svc.at), NoteKind::Equivalent, "names the built image; Isoloom names it");
        }
    }

    /// Fixed build args carry over; the ones read from the shell are noted.
    fn build_args(&mut self, svc: &Service, b: &Mapping, m: &mut Machine) {
        let args: Vec<(String, String)> = match b.get("args") {
            Some(Value::Mapping(a)) => a.iter().filter_map(|(k, val)| Some((k.as_str()?.to_string(), str_of(val)?))).collect(),
            Some(Value::Sequence(a)) => a
                .iter()
                .filter_map(str_of)
                .filter_map(|item| item.split_once('=').map(|(k, val)| (k.to_string(), val.to_string())))
                .collect(),
            _ => Vec::new(),
        };
        let (shell, fixed): (Vec<_>, Vec<_>) = args.into_iter().partition(|(_, val)| val.contains('$'));
        m.args.extend(fixed);
        if !shell.is_empty() {
            let shell: Vec<String> = shell.into_iter().map(|(k, _)| k).collect();
            self.notes.add(
                format!("{}.build.args", svc.at),
                NoteKind::InImage,
                format!("read from the shell, left out: {} (give fixed values)", shell.join(", ")),
            );
        }
    }

    fn ports(&mut self, kat: &str, published: bool, v: &Value, m: &mut Machine) {
        let ports = match container_ports(v) {
            Ok(ports) => ports,
            Err(bad) => {
                self.notes.add(
                    kat,
                    NoteKind::Changed,
                    format!("couldn't read port `{bad}`; add it to the machine's `ports` by hand"),
                );
                return;
            }
        };
        m.ports.extend(ports);
        let other = other_protocols(v);
        if !other.is_empty() {
            self.notes
                .add(kat, NoteKind::Changed, format!("{} drafted as TCP: a port is TCP or UDP", other.join(", ")));
        }
        if !published {
            return;
        }
        let (found, unknown) = published_ports(v);
        m.publish.extend(found);
        if !m.publish.is_empty() {
            self.notes.add(
                kat,
                NoteKind::Changed,
                "published on the host's loopback (127.0.0.1) only; Compose publishes on every interface unless told otherwise",
            );
        }
        if !unknown.is_empty() {
            self.notes.add(
                kat,
                NoteKind::Changed,
                format!("no fixed host port for {}: add `publish:` to the service by hand", unknown.join(", ")),
            );
        }
    }

    /// Values read from the shell become inputs; fixed values belong in the image.
    fn environment(&mut self, kat: String, v: &Value, m: &mut Machine) {
        let mut fixed = Vec::new();
        for (key, value) in env_entries(v) {
            let from_shell: Vec<String> = match &value {
                None => input_name(&key).into_iter().collect(),
                Some(v) => variables(v).iter().filter_map(|v| input_name(v)).collect(),
            };
            if from_shell.is_empty() {
                fixed.push(format!("{key}={}", value.unwrap_or_default()));
            }
            for i in from_shell {
                m.inputs.insert(i.clone());
                self.spec_inputs.insert(i);
            }
        }
        if !fixed.is_empty() {
            self.notes
                .add(kat, NoteKind::InImage, format!("fixed values belong in the image (ENV): {}", fixed.join(", ")));
        }
    }

    /// Resource limits carry over; the rest of `deploy` is noted.
    fn deploy(&mut self, kat: String, v: &Value, m: &mut Machine) {
        let limits = v.get("resources").and_then(|r| r.get("limits"));
        if let Some(c) = limits.and_then(|l| l.get("cpus")).and_then(cpus) {
            m.cpus = Some(c);
        }
        if let Some(mb) = limits.and_then(|l| l.get("memory")).and_then(str_of).and_then(|s| memory_mb(&s)) {
            m.memory_mb = Some(mb);
        }
        let other: Vec<&str> = v
            .as_mapping()
            .into_iter()
            .flatten()
            .filter_map(|(k, _)| k.as_str())
            .filter(|k| *k != "resources")
            .collect();
        if !other.is_empty() {
            self.notes
                .add(kat, NoteKind::NotYet, format!("only resource limits are kept; not: {}", other.join(", ")));
        }
    }

    /// A volume of one machine carries over; host paths and volumes shared between machines don't.
    fn volumes(&mut self, kat: String, v: &Value, m: &mut Machine) {
        let mut binds = Vec::new();
        let mut shared = Vec::new();
        for item in v.as_sequence().into_iter().flatten() {
            match volume_mount(item) {
                Some((VolumeMount::Bind, _)) | None => binds.push(flat(item)),
                Some((VolumeMount::Named(n), path)) if self.volume_users.get(&n).copied().unwrap_or(0) > 1 => shared.push(format!("{n}:{path}")),
                Some((VolumeMount::Named(n), path)) => m.volumes.push((label(&n), path)),
                Some((VolumeMount::Anonymous, path)) => m.volumes.push((label(path.trim_start_matches('/')), path)),
            }
        }
        if !binds.is_empty() {
            self.notes.add(
                kat.clone(),
                NoteKind::InImage,
                format!(
                    "project files mounted in: copy them into the image, or run scripts as `docker.init` jobs: {}",
                    binds.join(", ")
                ),
            );
        }
        if !shared.is_empty() {
            self.notes.add(
                kat,
                NoteKind::NotYet,
                format!("data shared between machines (not in the format yet): {}", shared.join(", ")),
            );
        }
    }
}

/// The build context, as a path in the project, and the build settings that don't carry over.
fn build_context(v: &Value) -> (String, Vec<String>) {
    let (context, extra) = match v {
        Value::Mapping(b) => (
            b.get("context").and_then(str_of).unwrap_or_else(|| ".".into()),
            b.iter()
                .filter_map(|(k, v)| Some((k.as_str()?, v)))
                // The context, the Dockerfile, its stage and fixed build args carry over.
                .filter(|(k, _)| !matches!(*k, "context" | "dockerfile" | "target" | "args"))
                .map(|(k, _)| k.to_string())
                .collect::<Vec<_>>(),
        ),
        other => (str_of(other).unwrap_or_else(|| ".".into()), Vec::new()),
    };
    let context = context.trim_start_matches("./").trim_end_matches('/').to_string();
    (if context.is_empty() { ".".to_string() } else { context }, extra)
}

/// The Dockerfile, when it isn't the default one, as a path in the project.
fn dockerfile(context: &str, b: &Mapping) -> Option<String> {
    let f = b.get("dockerfile").and_then(str_of).filter(|f| f != "Dockerfile")?;
    // Compose reads it relative to the context; the spec, from the project.
    let joined = if context == "." { f } else { format!("{context}/{f}") };
    // `src/../docker/x` -> `docker/x`: a path in the project.
    let mut parts: Vec<&str> = Vec::new();
    for c in joined.split('/') {
        match c {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    Some(parts.join("/"))
}
