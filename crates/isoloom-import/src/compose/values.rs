//! Reading Compose values: text, shell variables, ports, memory sizes and volume entries.

use serde_yaml_ng::Value;

use crate::model::Protocol;

pub(super) fn str_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A Compose value on one line, for notes.
pub(super) fn flat(v: &Value) -> String {
    match v {
        Value::Sequence(items) => items.iter().map(flat).collect::<Vec<_>>().join(" "),
        Value::Mapping(m) => m.iter().map(|(k, v)| format!("{}={}", flat(k), flat(v))).collect::<Vec<_>>().join(" "),
        other => str_of(other).unwrap_or_default(),
    }
}

/// Variables a Compose value reads from the shell: `${NAME}`, `${NAME:-x}`, `$NAME`.
pub(super) fn variables(s: &str) -> Vec<String> {
    let mut found = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' && i + 1 < b.len() {
            if b[i + 1] == b'$' {
                i += 2;
                continue;
            }
            let (start, braced) = if b[i + 1] == b'{' { (i + 2, true) } else { (i + 1, false) };
            let end = (start..b.len()).find(|&j| !(b[j].is_ascii_alphanumeric() || b[j] == b'_')).unwrap_or(b.len());
            if end > start && (!braced || end < b.len()) {
                found.push(s[start..end].to_string());
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    found
}

/// An input name from an environment variable name, as written (the shell reads it by that name).
pub(super) fn input_name(s: &str) -> Option<String> {
    let ok = !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !s.starts_with(|c: char| c.is_ascii_digit());
    ok.then(|| s.to_string())
}

/// The `environment` entries, mapping or list, as (name, value); `- NAME` alone has no value.
pub(super) fn env_entries(v: &Value) -> Vec<(String, Option<String>)> {
    match v {
        Value::Mapping(env) => env.iter().map(|(k, v)| (str_of(k).unwrap_or_default(), str_of(v))).collect(),
        Value::Sequence(list) => list
            .iter()
            .filter_map(str_of)
            .map(|e| match e.split_once('=') {
                Some((k, v)) => (k.to_string(), Some(v.to_string())),
                None => (e, None),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A container port and its protocol.
pub(super) type PortKey = (u16, Protocol);

/// The protocol a `ports` / `expose` entry names (`53/udp`, `{ protocol: udp }`), as written.
fn protocol_text(item: &Value) -> String {
    match item {
        Value::Mapping(m) => m.get("protocol").and_then(str_of).unwrap_or_default(),
        other => str_of(other)
            .unwrap_or_default()
            .rsplit_once('/')
            .map(|(_, p)| p.to_string())
            .unwrap_or_default(),
    }
}

/// The `ports` / `expose` entries with a protocol other than TCP and UDP (`sctp`), as written.
pub(super) fn other_protocols(v: &Value) -> Vec<String> {
    v.as_sequence()
        .into_iter()
        .flatten()
        .filter(|item| {
            let p = protocol_text(item);
            !p.is_empty() && !p.eq_ignore_ascii_case("tcp") && !p.eq_ignore_ascii_case("udp")
        })
        .map(flat)
        .collect()
}

/// The protocol of a `ports` / `expose` entry: `53/udp`, `{ protocol: udp }`, else TCP.
fn port_protocol(item: &Value) -> Protocol {
    if protocol_text(item).eq_ignore_ascii_case("udp") {
        Protocol::Udp
    } else {
        Protocol::Tcp
    }
}

/// Container ports from Compose `ports` (`"8080:80"`, `"127.0.0.1:8080:80/tcp"`, `"53:53/udp"`,
/// `80`, `{ target: 80, protocol: udp }`) and `expose`, with their protocol; ranges are expanded
/// up to 32 ports.
pub(super) fn container_ports(v: &Value) -> Result<Vec<PortKey>, String> {
    let mut out = Vec::new();
    for item in v.as_sequence().into_iter().flatten() {
        let proto = port_protocol(item);
        let spec = match item {
            Value::Mapping(m) => m.get("target").and_then(str_of).unwrap_or_default(),
            other => str_of(other).unwrap_or_default(),
        };
        let container = spec.rsplit(':').next().unwrap_or_default();
        let container = container.split('/').next().unwrap_or_default();
        let (lo, hi) = match container.split_once('-') {
            Some((a, b)) => (a.parse::<u16>(), b.parse::<u16>()),
            None => (container.parse::<u16>(), container.parse::<u16>()),
        };
        match (lo, hi) {
            (Ok(lo), Ok(hi)) if lo <= hi && hi - lo < 32 => out.extend((lo..=hi).map(|p| (p, proto))),
            _ => return Err(spec),
        }
    }
    Ok(out)
}

/// A host port as written: `${NAME:-8080}` -> 8080; a plain number as is.
fn host_port(h: &str) -> Option<u16> {
    let h = h.trim();
    match h.strip_prefix("${").and_then(|x| x.strip_suffix('}')) {
        Some(var) => var.split_once(":-").and_then(|(_, d)| d.parse().ok()),
        None => h.parse().ok(),
    }
}

/// The parts of a short `ports` entry, split on ':' outside `${...}`.
fn port_parts(spec: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut depth, mut cur) = (0, String::new());
    for ch in spec.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            ':' if depth == 0 => {
                parts.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    parts.push(cur);
    parts
}

/// The container port and fixed host port of one `ports` entry, when it says them.
fn published_port(item: &Value) -> (Option<u16>, Option<u16>) {
    match item {
        Value::Mapping(m) => (
            m.get("target").and_then(str_of).and_then(|t| t.parse::<u16>().ok()),
            m.get("published").and_then(str_of).and_then(|p| host_port(&p)),
        ),
        other => {
            let spec = str_of(other).unwrap_or_default();
            let parts = port_parts(spec.split('/').next().unwrap_or_default());
            let container = parts.last().and_then(|c| c.parse::<u16>().ok());
            let host = (parts.len() >= 2).then(|| host_port(&parts[parts.len() - 2])).flatten();
            (container, host)
        }
    }
}

/// Container port -> host port, for `ports` entries with a fixed host port (`8080:80`,
/// `127.0.0.1:8080:80`, `${WEB_PORT:-8080}:80`, `{ target: 80, published: 8080 }`); entries
/// with none (`80`, a variable without a default, a range) come back as text.
pub(super) fn published_ports(v: &Value) -> (Vec<(PortKey, u16)>, Vec<String>) {
    let mut found = Vec::new();
    let mut unknown = Vec::new();
    for item in v.as_sequence().into_iter().flatten() {
        match published_port(item) {
            (Some(c), Some(h)) if h > 0 => found.push(((c, port_protocol(item)), h)),
            _ => unknown.push(flat(item)),
        }
    }
    (found, unknown)
}

/// Memory like `512m`, `1g`, `1.5G`, `268435456` in MB.
pub(super) fn memory_mb(s: &str) -> Option<u32> {
    let s = s.trim().to_ascii_lowercase();
    let s = s.strip_suffix('b').unwrap_or(&s);
    let (num, factor) = match s.chars().last()? {
        'k' => (&s[..s.len() - 1], 1.0 / 1024.0),
        'm' => (&s[..s.len() - 1], 1.0),
        'g' => (&s[..s.len() - 1], 1024.0),
        _ => (s, 1.0 / (1024.0 * 1024.0)),
    };
    let mb = num.parse::<f64>().ok()? * factor;
    if mb >= 1.0 { crate::whole_u32(mb.ceil()) } else { None }
}

/// CPUs as a whole number, at least 1 (`"1.5"` -> 2).
pub(super) fn cpus(v: &Value) -> Option<u32> {
    str_of(v).and_then(|c| c.parse::<f64>().ok()).and_then(|c| crate::whole_u32(c.ceil().max(1.0)))
}

pub(super) enum VolumeMount {
    /// A named volume.
    Named(String),
    /// A path from the host (the project folder, most often).
    Bind,
    /// A volume with no name, at a path.
    Anonymous,
}

/// A service's volume entry (`name:/path[:ro]`, `./dir:/path`, `/path`, or the long syntax)
/// and the path in the container.
pub(super) fn volume_mount(item: &Value) -> Option<(VolumeMount, String)> {
    match item {
        Value::String(spec) => {
            let parts: Vec<&str> = spec.split(':').collect();
            match parts.as_slice() {
                [path] => Some((VolumeMount::Anonymous, path.to_string())),
                [source, path, ..] if source.starts_with('.') || source.starts_with('/') || source.starts_with('~') => {
                    Some((VolumeMount::Bind, path.to_string()))
                }
                [source, path, ..] => Some((VolumeMount::Named(source.to_string()), path.to_string())),
                [] => None,
            }
        }
        Value::Mapping(m) => {
            let target = m.get("target").and_then(str_of)?;
            match (m.get("type").and_then(str_of).as_deref(), m.get("source").and_then(str_of)) {
                (Some("volume"), Some(source)) => Some((VolumeMount::Named(source), target)),
                (Some("volume"), None) => Some((VolumeMount::Anonymous, target)),
                (Some("bind"), Some(_)) => Some((VolumeMount::Bind, target)),
                _ => None,
            }
        }
        _ => None,
    }
}
