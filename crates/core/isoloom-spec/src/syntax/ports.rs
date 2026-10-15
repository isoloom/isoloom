//! A machine's ports: a map keyed by name, each port a number (TCP) or `{ port, protocol,
//! publish, dormant }`. Messages and checks reach them by name
//! (`machines.web.ports.site.published`), so reordering never breaks a reference.

use std::fmt;
use std::num::NonZeroU16;

use schemars::JsonSchema;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

/// What a port speaks: TCP or UDP, and for TCP whether probes and checks speak HTTP or TLS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PortProtocol {
    /// TCP, the default: probed by connecting.
    #[default]
    Tcp,
    /// UDP (DNS, SNMP, TFTP): nothing waits for it and no derived check probes it.
    Udp,
    /// HTTP over TCP: derived checks request a page.
    Http,
    /// HTTPS over TCP: derived checks request a page over TLS, certificates unchecked.
    Https,
    /// TLS over TCP (not HTTP): probes speak TLS, certificates unchecked.
    Tls,
}

impl PortProtocol {
    /// Whether it runs over TCP (everything but UDP).
    pub fn is_tcp(self) -> bool {
        self != PortProtocol::Udp
    }

    /// Whether it speaks HTTP (`http`, `https`).
    pub fn is_http(self) -> bool {
        matches!(self, PortProtocol::Http | PortProtocol::Https)
    }

    /// Whether it speaks TLS (`https`, `tls`).
    pub fn is_tls(self) -> bool {
        matches!(self, PortProtocol::Https | PortProtocol::Tls)
    }
}

/// Whether and how a port is reachable from the user's machine (loopback only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Publish {
    /// `true`: a free host port, the port's own number when it is free.
    Free,
    /// A number: that host port when it is free, else a free one (labs never collide).
    Preferred(NonZeroU16),
    /// `{ host, fixed: true }`: that host port or nothing (an app that calls
    /// `localhost:<port>` itself); the environment can't start while another holds it.
    Fixed(NonZeroU16),
}

/// `publish:` as written, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)] // the schema's shape only: `Publish` reads the values.
enum PublishInput {
    /// `true`: a free host port, preferably the port's own number.
    Free(bool),
    /// The preferred host port.
    Preferred(NonZeroU16),
    /// A host port, and whether it is that one or nothing.
    Host {
        /// The host port.
        host: NonZeroU16,
        /// `true`: that port or nothing; `false` (the default): preferred.
        #[serde(default)]
        fixed: bool,
    },
}

/// A port a machine answers on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    /// The port the machine listens on. Left out only on a catalogue service's machine, whose
    /// entry gives it.
    pub port: Option<NonZeroU16>,
    /// What it speaks.
    pub protocol: PortProtocol,
    /// Whether it is published on the user's machine, and how.
    pub publish: Option<Publish>,
    /// It only answers later, once something wakes it (the bind shell an exploit opens):
    /// nothing waits for it and no derived check expects it to answer.
    pub dormant: bool,
}

/// A port written in full.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PortFull {
    /// The port the machine listens on (1 to 65535). On a catalogue service's machine, its
    /// entry gives it (change it through `with.port`).
    #[serde(default)]
    port: Option<NonZeroU16>,
    /// `tcp` (the default), `udp`, or TCP that speaks `http`, `https` or `tls`: probes and
    /// derived checks speak it.
    #[serde(default)]
    protocol: PortProtocol,
    /// Reachable from the user's machine (loopback only): `true` for a free host port
    /// (preferably the same number), a number for the preferred host port, or `{ host, fixed:
    /// true }` for that host port or nothing.
    #[serde(default, deserialize_with = "publish")]
    #[schemars(with = "Option<PublishInput>")]
    publish: Option<Publish>,
    /// Only answers once something wakes it: nothing waits for it.
    #[serde(default)]
    dormant: bool,
}

/// A port as written: its number (TCP), or in full.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)] // the schema's shape only.
enum PortInput {
    /// The port's number: TCP, not published.
    Number(NonZeroU16),
    /// The port in full.
    Full(PortFull),
}

impl<'de> Deserialize<'de> for Port {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Port, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Port;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a port number, or `{ port, protocol, publish, dormant }`")
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Port, E> {
                let port = u16::try_from(v)
                    .ok()
                    .and_then(NonZeroU16::new)
                    .ok_or_else(|| E::custom("a port is 1 to 65535"))?;
                Ok(Port {
                    port: Some(port),
                    protocol: PortProtocol::Tcp,
                    publish: None,
                    dormant: false,
                })
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Port, A::Error> {
                let f = PortFull::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(Port {
                    port: f.port,
                    protocol: f.protocol,
                    publish: f.publish,
                    dormant: f.dormant,
                })
            }
        }
        d.deserialize_any(V)
    }
}

/// `publish:`: `true`, a number, or `{ host, fixed }` (`false`: not published).
fn publish<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Publish>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Option<Publish>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("`true`, a host port, or `{ host, fixed: true }`")
        }
        fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
            Ok(v.then_some(Publish::Free))
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            host_port(v).map(|p| Some(Publish::Preferred(p)))
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Host {
                host: NonZeroU16,
                #[serde(default)]
                fixed: bool,
            }
            let h = Host::deserialize(de::value::MapAccessDeserializer::new(map))?;
            Ok(Some(if h.fixed { Publish::Fixed(h.host) } else { Publish::Preferred(h.host) }))
        }
    }
    d.deserialize_any(V)
}

fn host_port<E: de::Error>(v: u64) -> Result<NonZeroU16, E> {
    u16::try_from(v)
        .ok()
        .and_then(NonZeroU16::new)
        .ok_or_else(|| E::custom("a host port is 1 to 65535"))
}

impl JsonSchema for Port {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Port".into()
    }

    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        PortInput::json_schema(g)
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::syntax::names::PortName;

    fn ports(yaml: &str) -> Result<IndexMap<PortName, Port>, String> {
        serde_yaml_ng::from_str(yaml).map_err(|e| e.to_string())
    }

    #[test]
    fn a_port_is_a_number_or_written_in_full() {
        let p = ports("{ ssh: 22, site: { port: 8000, protocol: http, publish: 8080 }, shell: { port: 4444, dormant: true } }").unwrap();
        let get = |n: &str| &p[&PortName::parse(n).unwrap()];
        assert_eq!((get("ssh").port.map(NonZeroU16::get), get("ssh").protocol), (Some(22), PortProtocol::Tcp));
        assert_eq!(get("site").publish, Some(Publish::Preferred(NonZeroU16::new(8080).unwrap())));
        assert!(get("shell").dormant && get("site").protocol.is_http() && !get("site").protocol.is_tls());
    }

    #[test]
    fn publish_is_true_a_number_or_fixed() {
        let p = ports("{ a: { port: 1, publish: true }, b: { port: 2, publish: { host: 3206, fixed: true } }, c: { port: 3, publish: false } }").unwrap();
        let publish: Vec<_> = p.values().map(|p| p.publish).collect();
        assert_eq!(publish, [Some(Publish::Free), Some(Publish::Fixed(NonZeroU16::new(3206).unwrap())), None]);
    }

    #[test]
    fn mistakes_are_named() {
        assert!(ports("{ a: 0 }").unwrap_err().contains("1 to 65535"));
        assert!(ports("{ a: { port: 1, protocol: quic } }").unwrap_err().contains("unknown variant"));
        assert!(ports("{ a: { port: 1, http: true } }").unwrap_err().contains("unknown field `http`"));
        assert!(ports("{ 80: 80 }").unwrap_err().contains("port's name"));
    }
}
