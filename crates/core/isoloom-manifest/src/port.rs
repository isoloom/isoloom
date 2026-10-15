//! A port a machine answers on: its name, its number, what it speaks, and how the host publishes
//! it. What it speaks is one [`PortKind`], as the spec's `protocol:` names it; the JSON keeps the
//! contract's `protocol`, `http` and `tls`, and a UDP port that speaks HTTP or TLS doesn't parse.

use std::num::NonZeroU16;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::names::PortName;
use crate::template::Template;
use crate::{Error, Result};

/// A port's transport protocol.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// TCP.
    #[default]
    Tcp,
    /// UDP.
    Udp,
}

/// What a port speaks, as the spec's `protocol:` names it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum PortKind {
    /// TCP, nothing more known.
    #[default]
    Tcp,
    /// UDP.
    Udp,
    /// HTTP over TCP.
    Http,
    /// HTTPS: HTTP over TLS.
    Https,
    /// TLS that isn't HTTP.
    Tls,
}

impl PortKind {
    /// The kind of a port with this transport that speaks HTTP or TLS (the contract's
    /// `protocol`, `http` and `tls`).
    ///
    /// # Errors
    ///
    /// [`Error::PortProtocol`] for UDP with HTTP or TLS, which run over TCP.
    pub fn from_parts(protocol: Protocol, http: bool, tls: bool) -> Result<PortKind> {
        match (protocol, http, tls) {
            (Protocol::Udp, false, false) => Ok(PortKind::Udp),
            (Protocol::Udp, ..) => Err(Error::PortProtocol),
            (Protocol::Tcp, false, false) => Ok(PortKind::Tcp),
            (Protocol::Tcp, true, false) => Ok(PortKind::Http),
            (Protocol::Tcp, true, true) => Ok(PortKind::Https),
            (Protocol::Tcp, false, true) => Ok(PortKind::Tls),
        }
    }

    /// Its transport: UDP for `udp`, TCP for the others.
    #[must_use]
    pub const fn transport(self) -> Protocol {
        match self {
            PortKind::Udp => Protocol::Udp,
            PortKind::Tcp | PortKind::Http | PortKind::Https | PortKind::Tls => Protocol::Tcp,
        }
    }

    /// It speaks HTTP: `http` and `https`.
    #[must_use]
    pub const fn http(self) -> bool {
        matches!(self, PortKind::Http | PortKind::Https)
    }

    /// It speaks TLS: `https` and `tls`.
    #[must_use]
    pub const fn tls(self) -> bool {
        matches!(self, PortKind::Https | PortKind::Tls)
    }
}

/// How a port is published on the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PublishMode {
    /// Always `preferred`.
    Fixed,
    /// A free port, unless `ISOLOOM_PUBLISH_FIXED` is set; the runner pins it at the first start.
    Ephemeral,
}

/// A port published on the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Publish {
    /// Fixed or ephemeral.
    pub mode: PublishMode,
    /// The host port asked for (`publish:` in the spec).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred: Option<NonZeroU16>,
    /// The host address it listens on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<Template>,
}

/// A port a machine answers on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PortJson", into = "PortJson")]
pub struct Port {
    /// Its name in the spec (`portal`): unique on its machine, how placeholders and outputs
    /// name it (`${published.web.portal}`).
    pub name: PortName,
    /// The machine's port.
    pub port: NonZeroU16,
    /// What it speaks.
    pub kind: PortKind,
    /// Published on the host, and how.
    pub publish: Option<Publish>,
}

/// A port a machine answers on.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PortJson {
    /// Its name in the spec (`portal`): unique on its machine, how placeholders and outputs
    /// name it (`${published.web.portal}`).
    name: PortName,
    /// The machine's port.
    port: NonZeroU16,
    /// TCP or UDP.
    protocol: Protocol,
    /// It speaks HTTP.
    http: bool,
    /// It speaks TLS.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    tls: bool,
    /// Published on the host, and how.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publish: Option<Publish>,
}

impl TryFrom<PortJson> for Port {
    type Error = Error;

    fn try_from(p: PortJson) -> Result<Port> {
        Ok(Port {
            kind: PortKind::from_parts(p.protocol, p.http, p.tls)?,
            name: p.name,
            port: p.port,
            publish: p.publish,
        })
    }
}

impl From<Port> for PortJson {
    fn from(p: Port) -> PortJson {
        PortJson {
            name: p.name,
            port: p.port,
            protocol: p.kind.transport(),
            http: p.kind.http(),
            tls: p.kind.tls(),
            publish: p.publish,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [PortKind; 5] = [PortKind::Tcp, PortKind::Udp, PortKind::Http, PortKind::Https, PortKind::Tls];

    fn port(json: &str) -> Result<Port, serde_json::Error> {
        serde_json::from_str(&format!(r#"{{"name": "site", "port": 80, {json}}}"#))
    }

    #[test]
    fn each_kind_is_the_protocol_and_flags_it_reads_from() {
        for kind in KINDS {
            assert_eq!(PortKind::from_parts(kind.transport(), kind.http(), kind.tls()), Ok(kind), "{kind:?}");
        }
        let flags = KINDS.map(|k| (k.transport(), k.http(), k.tls()));
        assert_eq!(
            flags,
            [
                (Protocol::Tcp, false, false),
                (Protocol::Udp, false, false),
                (Protocol::Tcp, true, false),
                (Protocol::Tcp, true, true),
                (Protocol::Tcp, false, true),
            ]
        );
    }

    #[test]
    fn a_udp_port_speaks_neither_http_nor_tls() {
        for (http, tls) in [(true, false), (false, true), (true, true)] {
            assert_eq!(PortKind::from_parts(Protocol::Udp, http, tls), Err(Error::PortProtocol), "{http} {tls}");
        }
        let err = port(r#""protocol": "udp", "http": true"#).unwrap_err();
        assert!(err.to_string().contains("UDP"), "{err}");
    }

    #[test]
    fn a_port_reads_and_writes_the_contracts_fields() {
        let https = port(r#""protocol": "tcp", "http": true, "tls": true"#).unwrap();
        assert_eq!(https.kind, PortKind::Https);
        let json = serde_json::to_value(&https).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"name": "site", "port": 80, "protocol": "tcp", "http": true, "tls": true})
        );
        let plain = port(r#""protocol": "udp", "http": false"#).unwrap();
        assert_eq!(plain.kind, PortKind::Udp);
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            serde_json::json!({"name": "site", "port": 80, "protocol": "udp", "http": false})
        );
        assert!(port(r#""protocol": "tcp", "http": false, "speed": 1"#).is_err());
    }
}
