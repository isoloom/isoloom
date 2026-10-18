//! What a machine runs and when it is ready: its `command` (with the directory it runs in), its
//! `healthcheck` (a port by name or number), and its `environment` values.

use std::borrow::Cow;
use std::fmt;
use std::num::NonZeroU16;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::names::{InputName, PortName};
use crate::Error;
use crate::model::CommandLine;

/// A machine's own command: a text the shell runs, or a list run as it is, and the directory
/// it runs in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "CommandInput")]
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

// What the schema shows and the reader returns.
/// A command as written, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
enum CommandInput {
    /// A text run by the machine's shell (`/bin/sh -c`; PowerShell on Windows).
    Shell(String),
    /// Arguments run as they are, the program first.
    Exec(Vec<String>),
    /// The command and the directory it runs in.
    Full(CommandFull),
}

impl<'de> Deserialize<'de> for CommandInput {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<CommandInput, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = CommandInput;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a text, a list of arguments, or `{ run, cwd }`")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<CommandInput, E> {
                Ok(CommandInput::Shell(v.to_owned()))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<CommandInput, A::Error> {
                Vec::<String>::deserialize(de::value::SeqAccessDeserializer::new(seq)).map(CommandInput::Exec)
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<CommandInput, A::Error> {
                CommandFull::deserialize(de::value::MapAccessDeserializer::new(map)).map(CommandInput::Full)
            }
        }
        d.deserialize_any(V)
    }
}

impl From<CommandInput> for Command {
    fn from(written: CommandInput) -> Command {
        match written {
            CommandInput::Shell(text) => Command {
                run: CommandLine::Shell(text),
                cwd: None,
            },
            CommandInput::Exec(args) => Command {
                run: CommandLine::Exec(args),
                cwd: None,
            },
            CommandInput::Full(c) => Command { run: c.run, cwd: c.cwd },
        }
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

/// What a healthcheck tries: a command, or a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthProbe {
    /// A command run inside the machine: healthy when it exits with 0.
    Test(CommandLine),
    /// A page the machine serves: healthy when it answers with a 2xx or 3xx status.
    Http(HealthPage),
}

/// When a machine counts as healthy, besides its ports answering: a command that succeeds, or
/// a page that answers, tried at its own pace.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "HealthcheckInput")]
pub struct Healthcheck {
    /// What it tries.
    pub probe: HealthProbe,
    /// The time between two tries (`5s`, `1m30s`). Default: 5s.
    pub interval: Option<String>,
    /// How long one try may take. Default: 3s.
    pub timeout: Option<String>,
    /// Failed tries in a row before the machine counts as unhealthy. Default: 60.
    pub retries: Option<u32>,
    /// A grace period after start before the first try counts (`30s`). Default: 10s.
    pub start_period: Option<String>,
}

// A healthcheck as written, what the schema shows: exactly one of `test` and `http`.
/// When a machine counts as healthy, besides its ports answering: a command that succeeds, or
/// a page that answers.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct HealthcheckInput {
    /// A command run inside the machine: healthy when it exits with 0 (exactly one of `test`
    /// and `http`).
    #[serde(default)]
    test: Option<CommandLine>,
    /// A page the machine serves: healthy when it answers with a 2xx or 3xx status.
    #[serde(default)]
    http: Option<HealthPage>,
    /// The time between two tries (`5s`, `1m30s`). Default: 5s.
    #[serde(default)]
    interval: Option<String>,
    /// How long one try may take. Default: 3s.
    #[serde(default)]
    timeout: Option<String>,
    /// Failed tries in a row before the machine counts as unhealthy. Default: 60.
    #[serde(default)]
    retries: Option<u32>,
    /// A grace period after start before the first try counts (`30s`). Default: 10s.
    #[serde(default)]
    start_period: Option<String>,
}

impl TryFrom<HealthcheckInput> for Healthcheck {
    type Error = Error;

    fn try_from(h: HealthcheckInput) -> Result<Healthcheck, Error> {
        let probe = match (h.test, h.http) {
            (Some(test), None) => HealthProbe::Test(test),
            (None, Some(page)) => HealthProbe::Http(page),
            (Some(_), Some(_)) => return Err(Error::TestAndHttp),
            (None, None) => return Err(Error::NoHealthProbe),
        };
        Ok(Healthcheck {
            probe,
            interval: h.interval,
            timeout: h.timeout,
            retries: h.retries,
            start_period: h.start_period,
        })
    }
}

impl JsonSchema for Healthcheck {
    fn schema_name() -> Cow<'static, str> {
        "Healthcheck".into()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        HealthcheckInput::json_schema(g)
    }
}

/// An environment variable's value: a fixed text (a number or a boolean read as its text), or a
/// non-sensitive input's value.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "EnvInput")]
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

// What the schema shows and the reader returns.
/// An environment value as written, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
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

impl<'de> Deserialize<'de> for EnvInput {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<EnvInput, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = EnvInput;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a text, a number, a boolean, or `{ input: NAME }`")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<EnvInput, E> {
                Ok(EnvInput::Text(v.to_owned()))
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<EnvInput, E> {
                Ok(EnvInput::Bool(v))
            }
            // A whole number reads as its digits, exactly (through a float it could round).
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<EnvInput, E> {
                Ok(EnvInput::Text(v.to_string()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<EnvInput, E> {
                Ok(EnvInput::Text(v.to_string()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<EnvInput, E> {
                Ok(EnvInput::Number(v))
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<EnvInput, A::Error> {
                EnvReference::deserialize(de::value::MapAccessDeserializer::new(map)).map(EnvInput::Reference)
            }
        }
        d.deserialize_any(V)
    }
}

impl TryFrom<EnvInput> for EnvValue {
    type Error = Error;

    fn try_from(written: EnvInput) -> Result<EnvValue, Error> {
        match written {
            EnvInput::Text(text) => Ok(EnvValue::Text(text)),
            EnvInput::Number(n) => Ok(EnvValue::Text(n.to_string())),
            EnvInput::Bool(b) => Ok(EnvValue::Text(b.to_string())),
            EnvInput::Reference(EnvReference { input: Some(i), secret: None }) => Ok(EnvValue::Input(i)),
            EnvInput::Reference(EnvReference { input: None, secret: Some(s) }) => Ok(EnvValue::Secret(s)),
            EnvInput::Reference(_) => Err(Error::EnvReference),
        }
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
    fn a_command_or_an_environment_value_of_the_wrong_shape_says_what_is_expected() {
        let command = serde_yaml_ng::from_str::<Command>("5").unwrap_err().to_string();
        assert!(command.contains("a text, a list of arguments, or `{ run, cwd }`"), "{command}");
        let value = serde_yaml_ng::from_str::<EnvValue>("[1]").unwrap_err().to_string();
        assert!(value.contains("a text, a number, a boolean, or `{ input: NAME }`"), "{value}");
        let both = serde_yaml_ng::from_str::<EnvValue>("{ input: A, secret: b }").unwrap_err().to_string();
        assert!(both.contains("write one of `{ input: NAME }`"), "{both}");
        assert_eq!(serde_yaml_ng::from_str::<EnvValue>("1.5").ok(), Some(EnvValue::Text("1.5".into())));
        assert_eq!(serde_yaml_ng::from_str::<EnvValue>("-3").ok(), Some(EnvValue::Text("-3".into())));
    }

    #[test]
    fn a_command_is_a_text_a_list_or_with_its_directory() {
        let text: Command = serde_yaml_ng::from_str("python3 app.py").unwrap();
        assert_eq!(text.run, CommandLine::Shell("python3 app.py".into()));
        let list: Command = serde_yaml_ng::from_str("[nginx, -g, daemon off;]").unwrap();
        assert!(matches!(list.run, CommandLine::Exec(ref a) if a.len() == 3));
        let full: Command = serde_yaml_ng::from_str("{ run: [python3, app.py], cwd: /srv }").unwrap();
        assert_eq!(full.cwd.as_deref(), Some("/srv"));
    }

    #[test]
    fn a_healthcheck_names_its_port() {
        let by_name: Healthcheck = serde_yaml_ng::from_str("{ http: { port: site, path: /health } }").unwrap();
        assert!(matches!(by_name.probe, HealthProbe::Http(HealthPage { port: PortRef::Name(ref n), .. }) if n.as_str() == "site"));
        let by_number: Healthcheck = serde_yaml_ng::from_str("{ http: { port: 8000 } }").unwrap();
        assert!(matches!(by_number.probe, HealthProbe::Http(HealthPage { port: PortRef::Number(_), .. })));
    }

    #[test]
    fn a_healthcheck_tries_one_thing() {
        let test: Healthcheck = serde_yaml_ng::from_str("{ test: [pg_isready], interval: 2s }").unwrap();
        assert_eq!(
            (test.probe, test.interval.as_deref()),
            (HealthProbe::Test(CommandLine::Exec(vec!["pg_isready".into()])), Some("2s"))
        );
        let both = serde_yaml_ng::from_str::<Healthcheck>("{ test: x, http: { port: 80 } }")
            .unwrap_err()
            .to_string();
        assert!(both.contains("`test` and `http` are both given"), "{both}");
        let neither = serde_yaml_ng::from_str::<Healthcheck>("{ interval: 5s }").unwrap_err().to_string();
        assert!(neither.contains("a healthcheck says what to try"), "{neither}");
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
