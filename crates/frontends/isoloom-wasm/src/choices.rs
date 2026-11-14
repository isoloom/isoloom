//! What a drawing may say that Isoloom checks against lists of its own (the OSes, the network
//! appliances, the port protocols), read from Isoloom itself, so a caller offers exactly those.

use isoloom_spec::Appliance;
use isoloom_spec::syntax::PortProtocol;
use serde::{Deserialize, Serialize};

/// Isoloom's lists, as a topology writes their values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Choices {
    /// Every OS, `<family>@<version>` (`debian@12`).
    pub os: Vec<String>,
    /// Every network appliance (`cisco-iol`).
    pub appliances: Vec<String>,
    /// Every port protocol (`tcp`, `http`...).
    pub protocols: Vec<String>,
}

/// Isoloom's lists.
#[must_use]
pub(crate) fn choices() -> Choices {
    Choices {
        os: isoloom_spec::os::DISTROS
            .iter()
            .flat_map(|f| f.versions.iter().map(move |(v, _)| format!("{}@{v}", f.name)))
            .collect(),
        appliances: values(&schemars::schema_for!(Appliance)),
        protocols: values(&schemars::schema_for!(PortProtocol)),
    }
}

/// The values a unit-variant enum's JSON Schema allows: its `enum` strings, or each variant's
/// `const` (a documented variant is one schema of a `oneOf`).
fn values(schema: &schemars::Schema) -> Vec<String> {
    let mut out = Vec::new();
    collect(schema.as_value(), &mut out);
    out
}

fn collect(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                match (key.as_str(), value) {
                    ("const", serde_json::Value::String(s)) => out.push(s.clone()),
                    ("enum", serde_json::Value::Array(list)) => {
                        out.extend(list.iter().filter_map(|s| s.as_str().map(str::to_owned)));
                    }
                    _ => collect(value, out),
                }
            }
        }
        serde_json::Value::Array(list) => list.iter().for_each(|x| collect(x, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lists_are_isoloom_s() {
        let c = choices();
        assert!(
            c.os.contains(&"debian@12".to_owned()) && c.os.contains(&"windows-server@2022".to_owned()),
            "{:?}",
            c.os
        );
        assert!(c.os.iter().all(|os| os.parse::<isoloom_spec::syntax::OsRef>().is_ok()));
        assert_eq!(c.appliances.len(), 7, "{:?}", c.appliances);
        assert!(c.appliances.contains(&"cisco-vios-l2".to_owned()));
        assert_eq!(c.protocols, ["tcp", "udp", "http", "https", "tls"]);
    }
}
