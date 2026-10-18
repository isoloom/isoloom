//! The steps that run things: `service` (a daemon enabled at boot, or a container's process),
//! `command` (a command run once, behind its guard) and `firewall` (ports opened in the OS's own
//! firewall).

use std::borrow::Cow;
use std::fmt;
use std::num::NonZeroU16;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::super::mistake::Mistake;
use super::super::names::PortName;
use super::super::problem::Code;
use super::words::{AbsPath, UnitName, UserName};
use crate::model::Protocol;

/// A program and its arguments, run as they are (no shell): at least the program.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Argv(Vec<String>);

impl Argv {
    /// The program, then its arguments.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.0
    }

    /// The program.
    #[must_use]
    pub fn program(&self) -> &str {
        self.0.first().map_or("", String::as_str)
    }
}

impl TryFrom<Vec<String>> for Argv {
    type Error = String;

    /// The arguments, when the first names a program.
    ///
    /// # Errors
    ///
    /// Why not: an empty list, or an empty program.
    fn try_from(args: Vec<String>) -> Result<Argv, String> {
        match args.first() {
            Some(program) if !program.trim().is_empty() => Ok(Argv(args)),
            _ => Err("a command starts with its program (`[/usr/sbin/myd, --foreground]`)".to_owned()),
        }
    }
}

impl<'de> Deserialize<'de> for Argv {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Argv, D::Error> {
        Argv::try_from(Vec::<String>::deserialize(d)?).map_err(de::Error::custom)
    }
}

impl JsonSchema for Argv {
    fn schema_name() -> Cow<'static, str> {
        "Argv".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "array", "items": { "type": "string" }, "minItems": 1 })
    }
}

/// A command a step runs: a text the shell runs (`sh -c`; PowerShell on Windows), or a program
/// and its arguments run as they are.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Invocation {
    /// A text the shell runs, with its variables, pipes and `&&`.
    Shell(String),
    /// A program and its arguments, run as they are.
    Exec(Argv),
}

impl<'de> Deserialize<'de> for Invocation {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Invocation, D::Error> {
        d.deserialize_any(InvocationVisitor)
    }
}

/// A command as written: a text, or a list of arguments.
struct InvocationVisitor;

impl<'de> Visitor<'de> for InvocationVisitor {
    type Value = Invocation;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a text the shell runs, or a list of arguments")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Invocation, E> {
        if v.trim().is_empty() {
            return Err(E::custom("a command says what it runs"));
        }
        Ok(Invocation::Shell(v.to_owned()))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Invocation, A::Error> {
        Argv::deserialize(de::value::SeqAccessDeserializer::new(seq)).map(Invocation::Exec)
    }
}

impl JsonSchema for Invocation {
    fn schema_name() -> Cow<'static, str> {
        "Invocation".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "anyOf": [{ "type": "string", "minLength": 1 }, { "type": "array", "items": { "type": "string" }, "minItems": 1 }] })
    }
}

/// How a step tells it already ran.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Guard {
    /// It already ran when this path exists.
    Creates(AbsPath),
    /// It already ran when this check exits 0.
    Unless(Invocation),
}

/// A `command` step: a command run once, behind its guard.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandStep {
    /// What runs.
    pub run: Invocation,
    /// How it tells it already ran.
    pub guard: Guard,
}

/// A `service` step: a daemon enabled at boot and running (a VM), or run by the container (§3 of
/// the configuration model).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "ServiceInput")]
#[non_exhaustive]
pub enum Service {
    /// A unit a package installs (`nginx`), or a key of the daemon table.
    Unit {
        /// The unit.
        name: UnitName,
        /// Started at boot and now (`false`: disabled and stopped).
        enabled: bool,
    },
    /// A service Isoloom defines: a systemd or OpenRC unit, or a Windows service, on VMs; a
    /// process of the container's runner in containers.
    Defined {
        /// Its name.
        name: UnitName,
        /// What runs, in the foreground.
        run: Argv,
        /// The user it runs as (default root).
        user: Option<UserName>,
        /// The folder it runs in (absolute).
        cwd: Option<AbsPath>,
        /// Started at boot and now (`false`: defined, but disabled and stopped).
        enabled: bool,
    },
}

impl Service {
    /// The unit's name.
    #[must_use]
    pub fn name(&self) -> &UnitName {
        match self {
            Service::Unit { name, .. } | Service::Defined { name, .. } => name,
        }
    }

    /// Whether it starts at boot and now.
    #[must_use]
    pub fn enabled(&self) -> bool {
        match self {
            Service::Unit { enabled, .. } | Service::Defined { enabled, .. } => *enabled,
        }
    }
}

/// A service written in full.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ServiceFull {
    /// The unit's name (`nginx`), or the name of the service `run` defines.
    name: UnitName,
    /// Defines the service: the program and its arguments, in the foreground (`[/srv/api/bin/api,
    /// --port, "8000"]`). Without it, the unit comes from a package.
    #[serde(default)]
    run: Option<Argv>,
    /// With `run`: the user it runs as (default root).
    #[serde(default)]
    user: Option<UserName>,
    /// With `run`: the folder it runs in (absolute).
    #[serde(default)]
    cwd: Option<AbsPath>,
    /// Started at boot and now (default `true`); `false` leaves it disabled and stopped.
    #[serde(default)]
    enabled: Option<bool>,
}

// What the schema shows and the reader returns.
/// A service as written, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
enum ServiceInput {
    /// A unit a package installs (`nginx`), enabled and started.
    Unit(UnitName),
    /// A unit with its settings, or a service Isoloom defines from `run`.
    Full(ServiceFull),
}

schema_as!(Service, ServiceInput);

impl<'de> Deserialize<'de> for ServiceInput {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<ServiceInput, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = ServiceInput;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a unit's name, or `{ name, run, user, cwd, enabled }`")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<ServiceInput, E> {
                v.parse().map(ServiceInput::Unit).map_err(E::custom)
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<ServiceInput, A::Error> {
                ServiceFull::deserialize(de::value::MapAccessDeserializer::new(map)).map(ServiceInput::Full)
            }
        }
        d.deserialize_any(V)
    }
}

impl TryFrom<ServiceInput> for Service {
    type Error = Mistake;

    fn try_from(written: ServiceInput) -> Result<Service, Mistake> {
        let s = match written {
            ServiceInput::Unit(name) => return Ok(Service::Unit { name, enabled: true }),
            ServiceInput::Full(s) => s,
        };
        let enabled = s.enabled.unwrap_or(true);
        match s.run {
            Some(run) => Ok(Service::Defined {
                name: s.name,
                run,
                user: s.user,
                cwd: s.cwd,
                enabled,
            }),
            None if s.user.is_some() || s.cwd.is_some() => {
                let field = if s.user.is_some() { "user" } else { "cwd" };
                Err(
                    Mistake::new(Code::Conflict, format!("`{field}` goes with `run`: a package's unit runs as its package says"))
                        .at(field)
                        .fix("`run: [<program>, <args>...]` to define the service, or leave it out"),
                )
            }
            None => Ok(Service::Unit { name: s.name, enabled }),
        }
    }
}

/// A `firewall` step: ports opened in the OS's own firewall, when one runs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Firewall {
    /// The ports to open: the machine's port names (`http`), or `<number>` and `<number>/udp`
    /// (`8443`, `53/udp`). Allow rules only: deny rules are 1.x, and `reach` governs traffic
    /// between machines.
    pub allow: Vec<FirewallPort>,
}

/// A port a `firewall` step opens.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FirewallPort {
    /// One of the machine's ports, by name: its number and protocol.
    Named(PortName),
    /// A port by number.
    Number {
        /// The number.
        port: NonZeroU16,
        /// Its transport.
        protocol: Protocol,
    },
}

impl<'de> Deserialize<'de> for FirewallPort {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<FirewallPort, D::Error> {
        d.deserialize_any(PortVisitor)
    }
}

/// A port as written: a name, a number, or `<number>/udp` (or `/tcp`).
struct PortVisitor;

impl Visitor<'_> for PortVisitor {
    type Value = FirewallPort;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a port's name, a number, or `<number>/udp`")
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<FirewallPort, E> {
        number(&v.to_string(), Protocol::Tcp)
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<FirewallPort, E> {
        number(&v.to_string(), Protocol::Tcp)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<FirewallPort, E> {
        match v.split_once('/') {
            Some((n, "udp")) => number(n, Protocol::Udp),
            Some((n, "tcp")) => number(n, Protocol::Tcp),
            Some(_) => Err(E::custom(format!("`{v}`: a port is `<number>`, `<number>/udp` or `<number>/tcp`"))),
            None if v.starts_with(|c: char| c.is_ascii_digit()) => number(v, Protocol::Tcp),
            None => v.parse().map(FirewallPort::Named).map_err(E::custom),
        }
    }
}

/// Port number `n`, 1 to 65535.
fn number<E: de::Error>(n: &str, protocol: Protocol) -> Result<FirewallPort, E> {
    n.parse::<NonZeroU16>()
        .map(|port| FirewallPort::Number { port, protocol })
        .map_err(|_| E::custom(format!("`{n}` isn't a port: 1 to 65535")))
}

impl JsonSchema for FirewallPort {
    fn schema_name() -> Cow<'static, str> {
        "FirewallPort".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "anyOf": [
            { "type": "integer", "minimum": 1, "maximum": 65535 },
            { "type": "string", "pattern": "^([a-z][a-z0-9-]{0,30}|[0-9]{1,5}(/(udp|tcp))?)$" },
        ] })
    }
}
