//! What a machine runs and when it is ready: its `command` (with the directory it runs in), its
//! `healthcheck` (a port by name or number), and its `environment` values.

use std::borrow::Cow;
use std::fmt;
use std::num::NonZeroU16;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::names::{InputName, PortName};
use crate::model::CommandLine;

/// A machine's own command: a text the shell runs, or a list run as it is, and the directory
/// it runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// What runs.
    pub run: CommandLine,
    /// The directory it runs in (absolute), when not the image's `WORKDIR` (or the project
    /// folder on a VM).
    pub cwd: Option<String>,
}

/// A command written with its directory.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CommandFull {
    /// A text run by the shell, or a list of arguments run as they are.
    run: CommandLine,
    /// The directory the command, the healthcheck and the `setup` scripts run in (absolute).
    #[serde(default)]
    cwd: Option<String>,
}

/// A command as written, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)] // the schema's shape only.
enum CommandInput {
    /// A text run by the machine's shell (`/bin/sh -c`; PowerShell on Windows).
    Shell(String),
    /// Arguments run as they are, the program first.
    Exec(Vec<String>),
    /// The command and the directory it runs in.
    Full(CommandFull),
}

impl<'de> Deserialize<'de> for Command {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Command, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Command;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a text, a list of arguments, or `{ run, cwd }`")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Command, E> {
                Ok(Command {
                    run: CommandLine::Shell(v.to_string()),
                    cwd: None,
                })
            }
            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Command, A::Error> {
                let args = Vec::<String>::deserialize(de::value::SeqAccessDeserializer::new(seq))?;
                Ok(Command {
                    run: CommandLine::Exec(args),
                    cwd: None,
                })
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Command, A::Error> {
                let c = CommandFull::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(Command { run: c.run, cwd: c.cwd })
            }
        }
        d.deserialize_any(V)
    }
}

impl JsonSchema for Command {
    fn schema_name() -> Cow<'static, str> {
        "Command".into()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        CommandInput::json_schema(g)
    }
}

/// A port a healthcheck names: one of the machine's ports by name, or a port number.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PortRef {
    /// A port number.
    Number(NonZeroU16),
    /// One of the machine's ports, by name.
    Name(PortName),
}

/// A page a healthcheck requests on the machine itself.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HealthPage {
    /// The port it is served on: a declared port's name or a number.
    pub port: PortRef,
    /// The page's path. Default: `/`.
    #[serde(default = "root")]
    pub path: String,
}

fn root() -> String {
    "/".into()
}

/// When a machine counts as healthy, besides its ports answering: a command that succeeds, or
/// a page that answers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Healthcheck {
    /// A command run inside the machine: healthy when it exits with 0 (exactly one of `test`
    /// and `http`).
    #[serde(default)]
    pub test: Option<CommandLine>,
    /// A page the machine serves: healthy when it answers with a 2xx or 3xx status.
    #[serde(default)]
    pub http: Option<HealthPage>,
    /// The time between two tries (`5s`, `1m30s`). Default: 5s.
    #[serde(default)]
    pub interval: Option<String>,
    /// How long one try may take. Default: 3s.
    #[serde(default)]
    pub timeout: Option<String>,
    /// Failed tries in a row before the machine counts as unhealthy. Default: 60.
    #[serde(default)]
    pub retries: Option<u32>,
    /// A grace period after start before the first try counts (`30s`). Default: 10s.
    #[serde(default)]
    pub start_period: Option<String>,
}

/// An environment variable's value: a fixed text (a number or a boolean read as its text), or a
/// non-sensitive input's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvValue {
    /// A fixed value.
    Text(String),
    /// An input's value, given at each start (only a `sensitive: false` one).
    Input(InputName),
    /// A secret: refused (secrets are files), read so the problem can say so.
    Secret(String),
}

/// An environment value written as a reference.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EnvReference {
    /// A non-sensitive input, by name.
    #[serde(default)]
    input: Option<InputName>,
    /// Not allowed: a secret reaches a machine as a file (`files: [{ path, secret }]`).
    #[serde(default)]
    secret: Option<String>,
}

/// An environment value as written, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)] // the schema's shape only.
enum EnvInput {
    /// A fixed text.
    Text(String),
    /// A number, read as its text.
    Number(f64),
    /// A boolean, read as `true` or `false`.
    Bool(bool),
    /// `{ input: NAME }`: a non-sensitive input's value.
    Reference(EnvReference),
}

impl<'de> Deserialize<'de> for EnvValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<EnvValue, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = EnvValue;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a text, a number, a boolean, or `{ input: NAME }`")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<EnvValue, E> {
                Ok(EnvValue::Text(v.to_string()))
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<EnvValue, E> {
                Ok(EnvValue::Text(v.to_string()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<EnvValue, E> {
                Ok(EnvValue::Text(v.to_string()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<EnvValue, E> {
                Ok(EnvValue::Text(v.to_string()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<EnvValue, E> {
                Ok(EnvValue::Text(v.to_string()))
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<EnvValue, A::Error> {
                let r = EnvReference::deserialize(de::value::MapAccessDeserializer::new(map))?;
                match (r.input, r.secret) {
                    (Some(i), None) => Ok(EnvValue::Input(i)),
                    (None, Some(s)) => Ok(EnvValue::Secret(s)),
                    _ => Err(de::Error::custom("write one of `{ input: NAME }`")),
                }
            }
        }
        d.deserialize_any(V)
    }
}

impl JsonSchema for EnvValue {
    fn schema_name() -> Cow<'static, str> {
        "EnvValue".into()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        EnvInput::json_schema(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_a_text_a_list_or_with_its_directory() {
        let c: Command = serde_yaml_ng::from_str("python3 app.py").unwrap();
        assert_eq!(c.run, CommandLine::Shell("python3 app.py".into()));
        let c: Command = serde_yaml_ng::from_str("[nginx, -g, daemon off;]").unwrap();
        assert!(matches!(c.run, CommandLine::Exec(ref a) if a.len() == 3));
        let c: Command = serde_yaml_ng::from_str("{ run: [python3, app.py], cwd: /srv }").unwrap();
        assert_eq!(c.cwd.as_deref(), Some("/srv"));
    }

    #[test]
    fn a_healthcheck_names_its_port() {
        let h: Healthcheck = serde_yaml_ng::from_str("{ http: { port: site, path: /health } }").unwrap();
        assert_eq!(h.http.unwrap().port, PortRef::Name(PortName::parse("site").unwrap()));
        let h: Healthcheck = serde_yaml_ng::from_str("{ http: { port: 8000 } }").unwrap();
        assert!(matches!(h.http.unwrap().port, PortRef::Number(_)));
    }

    #[test]
    fn environment_values_are_scalars_or_inputs() {
        let v: indexmap::IndexMap<String, EnvValue> = serde_yaml_ng::from_str("{ A: x, B: 1024, C: true, D: { input: API_URL }, E: { secret: db } }").unwrap();
        let texts: Vec<_> = v.values().collect();
        assert_eq!(texts[1], &EnvValue::Text("1024".into()));
        assert!(matches!(texts[3], EnvValue::Input(_)) && matches!(texts[4], EnvValue::Secret(_)));
        assert!(serde_yaml_ng::from_str::<EnvValue>("{ file: x }").is_err());
    }
}
