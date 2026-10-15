//! Templates: strings holding `${root.path}` placeholders the runner fills at run time (`$$` is a
//! literal `$`). A template is parsed into its parts, so an unknown root, a lone `$` or a
//! `${secrets.…}` can't be held at all.

use std::borrow::Cow;
use std::fmt;
use std::num::NonZeroU16;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ManifestError;
use crate::machine::Protocol;
use crate::names::{EnvName, Name, SnakeName};

/// A value the runner knows about the run itself (`${run.…}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RunVar {
    /// The run's name (`${run.name}`): the embedder's, else the lab's name.
    Name,
    /// The manifest's folder, absolute (`${run.dir}`).
    Dir,
    /// The run's private folder (`${run.state_dir}`).
    StateDir,
    /// The Vagrant provider the run uses (`${run.provider}`).
    Provider,
    /// The runner's SSH key (`${run.ssh_key}`).
    SshKey,
    /// The run's own `known_hosts` (`${run.known_hosts}`).
    KnownHosts,
}

impl RunVar {
    const ALL: [RunVar; 6] = [
        RunVar::Name,
        RunVar::Dir,
        RunVar::StateDir,
        RunVar::Provider,
        RunVar::SshKey,
        RunVar::KnownHosts,
    ];

    /// The path after `run.`.
    pub fn key(self) -> &'static str {
        match self {
            RunVar::Name => "name",
            RunVar::Dir => "dir",
            RunVar::StateDir => "state_dir",
            RunVar::Provider => "provider",
            RunVar::SshKey => "ssh_key",
            RunVar::KnownHosts => "known_hosts",
        }
    }
}

/// One `${…}` placeholder.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Placeholder {
    /// `${run.…}`.
    Run(RunVar),
    /// `${params.NAME}`: a param's value.
    Param(EnvName),
    /// `${outputs.name}`: an output, once it can be read.
    Output(SnakeName),
    /// `${published.<machine>.<port>[.udp]}`: the host port a machine's port got.
    Published {
        /// The machine.
        machine: Name,
        /// Its port.
        port: NonZeroU16,
        /// The port's protocol (`.udp` in the placeholder).
        protocol: Protocol,
    },
    /// `${machine}`: the machine of a `lifecycle.provision.machine` step.
    Machine,
    /// `${value}`: an output's raw value, in its `format`.
    Value,
}

impl Placeholder {
    /// Reads what is between `${` and `}` (`template` is the whole text, for the error).
    fn parse(inner: &str, template: &str) -> Result<Placeholder, ManifestError> {
        let trimmed = inner.trim();
        if trimmed == "secrets" || trimmed.starts_with("secrets.") {
            return Err(ManifestError::SecretPlaceholder {
                template: template.to_string(),
            });
        }
        let unknown = || ManifestError::UnknownPlaceholder {
            template: template.to_string(),
            placeholder: inner.to_string(),
        };
        if let Some(key) = inner.strip_prefix("run.") {
            return RunVar::ALL.into_iter().find(|v| v.key() == key).map(Placeholder::Run).ok_or_else(unknown);
        }
        if let Some(name) = inner.strip_prefix("params.") {
            return EnvName::parse(name).map(Placeholder::Param).map_err(|_| unknown());
        }
        if let Some(name) = inner.strip_prefix("outputs.") {
            return SnakeName::parse(name).map(Placeholder::Output).map_err(|_| unknown());
        }
        if let Some(rest) = inner.strip_prefix("published.") {
            return published(rest).ok_or_else(unknown);
        }
        match inner {
            "machine" => Ok(Placeholder::Machine),
            "value" => Ok(Placeholder::Value),
            _ => Err(unknown()),
        }
    }

    /// The root, as validation names it (`run`, `params`, `machine`).
    pub fn root(&self) -> &'static str {
        match self {
            Placeholder::Run(_) => "run",
            Placeholder::Param(_) => "params",
            Placeholder::Output(_) => "outputs",
            Placeholder::Published { .. } => "published",
            Placeholder::Machine => "machine",
            Placeholder::Value => "value",
        }
    }
}

/// `<machine>.<port>[.udp]`, after `published.`.
fn published(rest: &str) -> Option<Placeholder> {
    let (rest, protocol) = match rest.strip_suffix(".udp") {
        Some(r) => (r, Protocol::Udp),
        None => (rest, Protocol::Tcp),
    };
    let (machine, port) = rest.rsplit_once('.')?;
    if !port.chars().all(|c| c.is_ascii_digit()) || port.starts_with('0') {
        return None;
    }
    Some(Placeholder::Published {
        machine: Name::parse(machine).ok()?,
        port: port.parse().ok()?,
        protocol,
    })
}

impl fmt::Display for Placeholder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Placeholder::Run(v) => write!(f, "run.{}", v.key()),
            Placeholder::Param(n) => write!(f, "params.{n}"),
            Placeholder::Output(n) => write!(f, "outputs.{n}"),
            Placeholder::Published { machine, port, protocol } => {
                write!(f, "published.{machine}.{port}")?;
                if *protocol == Protocol::Udp {
                    f.write_str(".udp")?;
                }
                Ok(())
            }
            Placeholder::Machine => f.write_str("machine"),
            Placeholder::Value => f.write_str("value"),
        }
    }
}

/// A piece of a template: literal text (unescaped), or a placeholder.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Part {
    /// Text, as it reads once filled (a `$` here is written `$$`).
    Text(String),
    /// A placeholder.
    Placeholder(Placeholder),
}

/// A string that may hold `${placeholder}`s (`$$` is a literal `$`). There is no `secrets.`
/// root: a secret reaches a tool only through its slot's delivery, so no value ever sits on a
/// command line.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Template {
    /// Never two texts in a row, never an empty text: one spelling, one value.
    parts: Vec<Part>,
}

impl Template {
    /// The template written as `text`.
    pub fn parse(text: &str) -> Result<Template, ManifestError> {
        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut rest = text;
        while let Some(at) = rest.find('$') {
            literal.push_str(&rest[..at]);
            let after = &rest[at + 1..];
            if let Some(r) = after.strip_prefix('$') {
                literal.push('$');
                rest = r;
                continue;
            }
            let Some(open) = after.strip_prefix('{') else {
                return Err(ManifestError::LoneDollar { template: text.to_string() });
            };
            let Some(end) = open.find('}') else {
                return Err(ManifestError::UnclosedPlaceholder { template: text.to_string() });
            };
            parts.push(Part::Text(std::mem::take(&mut literal)));
            parts.push(Part::Placeholder(Placeholder::parse(&open[..end], text)?));
            rest = &open[end + 1..];
        }
        literal.push_str(rest);
        parts.push(Part::Text(literal));
        Ok(Template::from_parts(parts))
    }

    /// Literal text, with no placeholder.
    pub fn text(text: &str) -> Template {
        Template::from_parts(vec![Part::Text(text.to_string())])
    }

    /// The template of `parts`, in order.
    pub fn from_parts(parts: impl IntoIterator<Item = Part>) -> Template {
        let mut out: Vec<Part> = Vec::new();
        for part in parts {
            match (out.last_mut(), part) {
                (_, Part::Text(t)) if t.is_empty() => {}
                (Some(Part::Text(last)), Part::Text(t)) => last.push_str(&t),
                (_, p) => out.push(p),
            }
        }
        Template { parts: out }
    }

    /// Its parts, in order.
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    /// Its placeholders, in order.
    pub fn placeholders(&self) -> impl Iterator<Item = &Placeholder> {
        self.parts.iter().filter_map(|p| match p {
            Part::Placeholder(p) => Some(p),
            Part::Text(_) => None,
        })
    }
}

impl From<Placeholder> for Template {
    fn from(p: Placeholder) -> Template {
        Template {
            parts: vec![Part::Placeholder(p)],
        }
    }
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for part in &self.parts {
            match part {
                Part::Text(t) => f.write_str(&t.replace('$', "$$"))?,
                Part::Placeholder(p) => write!(f, "${{{p}}}")?,
            }
        }
        Ok(())
    }
}

impl Serialize for Template {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Template {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Template, D::Error> {
        Template::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// A port from 1 to 65535, as a regular expression.
const PORT: &str = "(?:[1-9][0-9]{0,3}|[1-5][0-9]{4}|6[0-4][0-9]{3}|65[0-4][0-9]{2}|655[0-2][0-9]|6553[0-5])";

/// The template grammar as a regular expression: literal text, `$$`, and the known placeholders.
pub(crate) fn grammar() -> String {
    let roots = [
        "run\\.(?:name|dir|state_dir|provider|ssh_key|known_hosts)".to_string(),
        "params\\.[A-Za-z_][A-Za-z0-9_]*".to_string(),
        "outputs\\.[a-z][a-z0-9_]*".to_string(),
        format!("published\\.[A-Za-z0-9][A-Za-z0-9_.-]*\\.{PORT}(?:\\.udp)?"),
        "machine".to_string(),
        "value".to_string(),
    ];
    format!("^(?:[^$]|\\$\\$|\\$\\{{(?:{})\\}})*$", roots.join("|"))
}

impl JsonSchema for Template {
    fn schema_name() -> Cow<'static, str> {
        "Template".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A string that may hold `${placeholder}`s (`$$` is a literal `$`). Roots: `run.name`, `run.dir` (the manifest's folder), `run.state_dir`, `run.provider`, `run.ssh_key`, `run.known_hosts`, `params.<NAME>`, `outputs.<name>`, `published.<machine>.<port>[.udp]`, `machine` (a per-machine step), `value` (an output's own `format`). There is no `secrets.` root: a secret reaches a tool only through its slot's delivery, so no value ever sits on a command line.",
            "type": "string",
            "pattern": grammar(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(text: &str) -> Template {
        let t = Template::parse(text).unwrap();
        assert_eq!(t.to_string(), text);
        t
    }

    #[test]
    fn placeholders_and_text_are_told_apart() {
        let t = round_trip("${run.state_dir}/ports.yml");
        assert_eq!(
            t.parts(),
            [Part::Placeholder(Placeholder::Run(RunVar::StateDir)), Part::Text("/ports.yml".into())]
        );
        let t = round_trip("http://localhost:${published.web.80}/ and ${published.dns.53.udp}");
        assert_eq!(t.placeholders().count(), 2);
        round_trip("{{.ServerVersion}}");
        round_trip("costs $$5");
    }

    #[test]
    fn secrets_are_refused_whatever_the_spacing() {
        for text in ["${secrets.FLAG}", "a ${ secrets.x } b", "${secrets}"] {
            assert_eq!(Template::parse(text).unwrap_err().code(), "secret_placeholder", "{text}");
        }
    }

    #[test]
    fn malformed_templates_say_why() {
        assert_eq!(Template::parse("$5").unwrap_err().code(), "lone_dollar");
        assert_eq!(Template::parse("${run.name").unwrap_err().code(), "unclosed_placeholder");
        for unknown in ["${run.title}", "${env.HOME}", "${published.web.0}", "${published.web.65536}", "${outputs.Web}"] {
            assert_eq!(Template::parse(unknown).unwrap_err().code(), "unknown_placeholder", "{unknown}");
        }
    }

    #[test]
    fn parts_are_kept_in_one_spelling() {
        let built = Template::from_parts([Part::Text("a".into()), Part::Text(String::new()), Part::Text("$b".into())]);
        assert_eq!(built, Template::parse("a$$b").unwrap());
        assert_eq!(built.to_string(), "a$$b");
    }
}
