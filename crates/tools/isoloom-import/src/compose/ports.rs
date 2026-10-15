//! Reading Compose's `ports` and `expose`: the container ports with their protocol, and the host
//! ports they're published on.

use serde_yaml_ng::Value;

use super::text::{flat, str_of};
use isoloom_spec::model::Protocol;

/// A container port and its protocol.
pub(super) type PortKey = (u16, Protocol);

/// The most ports a range (`8000-8031`) is expanded to.
const RANGE_MAX: u16 = 32;

/// The protocol a `ports` / `expose` entry names (`53/udp`, `{ protocol: udp }`), as written.
fn protocol_text(item: &Value) -> String {
    match item {
        Value::Mapping(m) => m.get("protocol").and_then(str_of).unwrap_or_default(),
        other => str_of(other)
            .unwrap_or_default()
            .rsplit_once('/')
            .map(|(_, p)| p.to_owned())
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

/// The container ports of one entry's text (`8080:80/tcp`, `8000-8003`), or none when it says
/// no port or too wide a range.
fn container_range(entry: &str) -> Option<std::ops::RangeInclusive<u16>> {
    let container = entry.rsplit(':').next().unwrap_or_default();
    let container = container.split('/').next().unwrap_or_default();
    let (lo, hi) = container.split_once('-').unwrap_or((container, container));
    let (lo, hi) = (lo.parse::<u16>().ok()?, hi.parse::<u16>().ok()?);
    (lo <= hi && hi - lo < RANGE_MAX).then_some(lo..=hi)
}

/// Container ports from Compose `ports` (`"8080:80"`, `"127.0.0.1:8080:80/tcp"`, `"53:53/udp"`,
/// `80`, `{ target: 80, protocol: udp }`) and `expose`, with their protocol; ranges are expanded
/// up to 32 ports. An entry that says no port comes back as the error, as written.
pub(super) fn container_ports(v: &Value) -> Result<Vec<PortKey>, String> {
    let mut out = Vec::new();
    for item in v.as_sequence().into_iter().flatten() {
        let proto = port_protocol(item);
        let entry = match item {
            Value::Mapping(m) => m.get("target").and_then(str_of).unwrap_or_default(),
            other => str_of(other).unwrap_or_default(),
        };
        let range = container_range(&entry).ok_or(entry)?;
        out.extend(range.map(|p| (p, proto)));
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
fn port_parts(entry: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut depth, mut cur) = (0_u32, String::new());
    for ch in entry.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
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
            let entry = str_of(other).unwrap_or_default();
            let parts = port_parts(entry.split('/').next().unwrap_or_default());
            let mut from_the_end = parts.iter().rev();
            let container = from_the_end.next().and_then(|c| c.parse::<u16>().ok());
            let host = from_the_end.next().and_then(|h| host_port(h));
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

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn yaml(text: &str) -> Result<Value, serde_yaml_ng::Error> {
        serde_yaml_ng::from_str(text)
    }

    #[test]
    fn a_range_is_expanded_and_a_wide_one_refused() -> Result<(), serde_yaml_ng::Error> {
        let tcp = |p| (p, Protocol::Tcp);
        assert_eq!(container_ports(&yaml("[\"8000-8002:9000-9002\"]")?), Ok(vec![tcp(9000), tcp(9001), tcp(9002)]));
        assert_eq!(container_ports(&yaml("[\"1-100\"]")?), Err("1-100".to_owned()));
        Ok(())
    }

    #[test]
    fn a_host_port_with_a_default_is_that_default() {
        assert_eq!(port_parts("127.0.0.1:${P:-80}:80"), ["127.0.0.1", "${P:-80}", "80"]);
        assert_eq!(host_port("${P:-8080}"), Some(8080));
        assert_eq!(host_port("${P}"), None);
    }

    proptest! {
        #[test]
        fn every_short_form_of_a_published_port_is_read(host in 1_u16.., container in 1_u16.., udp in any::<bool>(), on_loopback in any::<bool>()) {
            let proto = if udp { "/udp" } else { "" };
            let ip = if on_loopback { "127.0.0.1:" } else { "" };
            let entry = format!("{ip}{host}:{container}{proto}");
            let v = Value::Sequence(vec![Value::String(entry.clone())]);
            let want = (container, if udp { Protocol::Udp } else { Protocol::Tcp });
            prop_assert_eq!(published_ports(&v), (vec![(want, host)], vec![]), "{}", entry);
            prop_assert_eq!(container_ports(&v), Ok(vec![want]));
        }

        #[test]
        fn an_entry_never_breaks_the_reading(entry in ".{0,30}") {
            let v = Value::Sequence(vec![Value::String(entry)]);
            let _ignored = (container_ports(&v), published_ports(&v), other_protocols(&v));
        }
    }
}
