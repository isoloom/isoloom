//! A machine's ports in the snapshot, by name: what a message's `{{ machines.<m>.ports.<name>.* }}`
//! placeholders read (`port`, `published`, `url`).

use serde_json::{Map, Value, json};

use isoloom_spec::model::{Machine, Port, Protocol, Spec};

/// What a port speaks, as the spec writes it (`tcp`, `udp`, `http`, `https`, `tls`).
fn protocol(p: &Port) -> &'static str {
    match (p.protocol, p.http, p.tls) {
        (Protocol::Udp, ..) => "udp",
        (_, true, true) => "https",
        (_, true, false) => "http",
        (_, false, true) => "tls",
        (_, false, false) => "tcp",
    }
}

/// The page of an HTTP or HTTPS port published on host port `host`, at the host's loopback.
fn url(p: &Port, host: u16) -> Option<String> {
    p.http.then(|| format!("{}://127.0.0.1:{host}/", if p.tls { "https" } else { "http" }))
}

/// A machine's ports, by name: each its number, protocol, and when published the host port it
/// asks for, the one it got (the same until a runner says) and its page's URL.
pub(super) fn of(m: &Machine) -> Map<String, Value> {
    m.ports.iter().map(|p| (m.port_label(p), port(p))).collect()
}

fn port(p: &Port) -> Value {
    let mut v = Map::new();
    v.insert("port".to_owned(), json!(p.port));
    v.insert("protocol".to_owned(), json!(protocol(p)));
    if let Some(host) = p.publish {
        v.insert("publish".to_owned(), json!(host));
        v.insert("published".to_owned(), json!(host));
        if let Some(u) = url(p, host) {
            v.insert("url".to_owned(), json!(u));
        }
    }
    for (key, set) in [("fixed", p.fixed), ("dormant", p.dormant)] {
        if set {
            v.insert(key.to_owned(), json!(true));
        }
    }
    Value::Object(v)
}

/// A port of the snapshot as a table or a diagram shows it: `53`, or `53/udp` for a UDP one.
pub(crate) fn shown(port: &Value) -> String {
    let number = port.get("port").map_or_else(String::new, ToString::to_string);
    if port.get("protocol").and_then(Value::as_str) == Some("udp") {
        format!("{number}/udp")
    } else {
        number
    }
}

/// Every published port: its machine, name, port and host port.
pub(super) fn published(spec: &Spec) -> Vec<Value> {
    spec.machines
        .iter()
        .flat_map(|(name, m)| {
            m.ports.iter().filter_map(move |p| {
                p.publish
                    .map(|host| json!({ "machine": name, "name": m.port_label(p), "port": p.port, "protocol": protocol(p), "host_port": host }))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_udp_port_is_shown_with_its_protocol() {
        assert_eq!(shown(&json!({ "port": 53, "protocol": "udp" })), "53/udp");
        assert_eq!(shown(&json!({ "port": 80, "protocol": "http" })), "80");
    }
}
