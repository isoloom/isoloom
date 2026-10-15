//! Commands: a program and its arguments, run without a shell. The program is a bare tool name
//! the runner resolves on PATH, and never a shell or an interpreter: the manifest holds no shell
//! code for the host.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::template::Template;
use crate::{Error, Result};

/// Programs that would run code given as an argument (shells, interpreters, privilege and
/// environment wrappers), refused whatever their case and with or without `.exe`.
const SHELLS: [&str; 24] = [
    "sh",
    "bash",
    "zsh",
    "dash",
    "ash",
    "ksh",
    "csh",
    "tcsh",
    "fish",
    "busybox",
    "cmd",
    "powershell",
    "pwsh",
    "wsl",
    "env",
    "sudo",
    "doas",
    "su",
    "python",
    "python2",
    "python3",
    "perl",
    "ruby",
    "node",
];

/// A step's program: a bare tool name (`docker`, `vagrant`), never a path, a placeholder or a
/// shell.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Program(String);

impl Program {
    /// The program, when it is a bare tool name and not a shell.
    ///
    /// # Errors
    ///
    /// [`Error::ProgramNotBare`] for a path, an option, a placeholder or anything but a
    /// tool's name; [`Error::ShellProgram`] for a shell or an interpreter.
    pub fn parse(name: &str) -> Result<Program> {
        let mut chars = name.chars();
        let bare = chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
        if !bare {
            return Err(Error::ProgramNotBare { program: name.to_owned() });
        }
        let lower = name.to_ascii_lowercase();
        let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
        if SHELLS.contains(&stem) {
            return Err(Error::ShellProgram { program: name.to_owned() });
        }
        Ok(Program(name.to_owned()))
    }

    /// The tool's name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Program {
    type Err = Error;

    fn from_str(name: &str) -> Result<Program> {
        Program::parse(name)
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Program {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Program {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Program, D::Error> {
        Program::parse(&String::deserialize(d)?).map_err(de::Error::custom)
    }
}

/// `name` matched whatever its case: each letter as a `[xX]` class.
fn any_case(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                format!("[{}{}]", c.to_ascii_lowercase(), c.to_ascii_uppercase())
            } else {
                c.to_string()
            }
        })
        .collect()
}

impl JsonSchema for Program {
    fn schema_name() -> Cow<'static, str> {
        "Program".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let shells: Vec<String> = SHELLS.iter().map(|s| any_case(s)).collect();
        json_schema!({
            "description": "A bare tool name resolved on PATH (with its Windows wrapper when it has one). A shell or an interpreter is refused, whatever its case and with or without `.exe`.",
            "type": "string",
            "pattern": "^[A-Za-z][A-Za-z0-9_.-]*$",
            "not": { "pattern": format!("^(?:{})(?:{})?$", shells.join("|"), any_case(".exe").replace('.', "\\.")) },
        })
    }
}

/// A program and its arguments, run without a shell; each argument is a template.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Argv {
    program: Program,
    args: Vec<Template>,
}

impl Argv {
    /// `program` with `args`.
    #[must_use]
    pub fn new(program: Program, args: Vec<Template>) -> Argv {
        Argv { program, args }
    }

    /// The program.
    #[must_use]
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// The arguments, in order.
    #[must_use]
    pub fn args(&self) -> &[Template] {
        &self.args
    }
}

/// One word of an argv as written: the program first, then each argument.
#[derive(Serialize)]
#[serde(untagged)]
enum Word<'a> {
    /// The program.
    Program(&'a Program),
    /// An argument.
    Arg(&'a Template),
}

impl Serialize for Argv {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(std::iter::once(Word::Program(&self.program)).chain(self.args.iter().map(Word::Arg)))
    }
}

impl<'de> Deserialize<'de> for Argv {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Argv, D::Error> {
        d.deserialize_seq(ArgvVisitor)
    }
}

struct ArgvVisitor;

impl<'de> Visitor<'de> for ArgvVisitor {
    type Value = Argv;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a program and its arguments, as a list of strings")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Argv, A::Error> {
        let program: Program = seq.next_element()?.ok_or_else(|| de::Error::custom(Error::EmptyArgv))?;
        let mut args = Vec::new();
        while let Some(arg) = seq.next_element()? {
            args.push(arg);
        }
        Ok(Argv { program, args })
    }
}

impl JsonSchema for Argv {
    fn schema_name() -> Cow<'static, str> {
        "Argv".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A program and its arguments, run without a shell.",
            "type": "array",
            "minItems": 1,
            "prefixItems": [generator.subschema_for::<Program>()],
            "items": generator.subschema_for::<Template>(),
        })
    }
}

/// One argv per host operating system, where hosts truly differ; `default` covers the hosts not
/// named.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("minProperties" = 1))]
pub struct PerHostArgv {
    /// For the hosts not named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Argv>,
    /// On Linux.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux: Option<Argv>,
    /// On macOS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos: Option<Argv>,
    /// On Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<Argv>,
}

/// A command: one argv for every host, or one per host operating system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostArgv {
    /// The same on every host.
    Every(Argv),
    /// Per host (at least one named).
    PerHost(PerHostArgv),
}

impl Serialize for HostArgv {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            HostArgv::Every(a) => a.serialize(s),
            HostArgv::PerHost(p) => p.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for HostArgv {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<HostArgv, D::Error> {
        d.deserialize_any(HostArgvVisitor)
    }
}

struct HostArgvVisitor;

impl<'de> Visitor<'de> for HostArgvVisitor {
    type Value = HostArgv;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an argv (a list), or one argv per host (`default`, `linux`, `macos`, `windows`)")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<HostArgv, A::Error> {
        Argv::deserialize(de::value::SeqAccessDeserializer::new(seq)).map(HostArgv::Every)
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<HostArgv, A::Error> {
        let per_host = PerHostArgv::deserialize(de::value::MapAccessDeserializer::new(map))?;
        if [&per_host.default, &per_host.linux, &per_host.macos, &per_host.windows]
            .iter()
            .all(|a| a.is_none())
        {
            return Err(de::Error::custom(Error::NoHostArgv));
        }
        Ok(HostArgv::PerHost(per_host))
    }
}

impl JsonSchema for HostArgv {
    fn schema_name() -> Cow<'static, str> {
        "HostArgv".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "One argv for every host, or one per host operating system (`default` covers the hosts not named). Only where a host truly differs.",
            "oneOf": [generator.subschema_for::<Argv>(), generator.subschema_for::<PerHostArgv>()],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_a_bare_tool_name() {
        assert!(Program::parse("docker").is_ok());
        assert!(Program::parse("VBoxManage").is_ok());
        for bad in ["/usr/bin/docker", "./docker", "${run.name}", "-rf", "", "docker compose"] {
            assert_eq!(Program::parse(bad).unwrap_err().code(), "program_not_bare", "{bad}");
        }
    }

    #[test]
    fn shells_are_refused_whatever_the_spelling() {
        for shell in ["sh", "BASH", "cmd.exe", "PowerShell.EXE", "pwsh", "env", "sudo", "python3"] {
            assert_eq!(Program::parse(shell).unwrap_err().code(), "shell_program", "{shell}");
        }
        assert!(Program::parse("shellcheck").is_ok());
    }

    #[test]
    fn a_program_converts_from_and_to_its_name() {
        let docker: Program = "docker".parse().unwrap();
        assert_eq!((docker.as_str(), docker.to_string()), ("docker", "docker".to_owned()));
    }

    #[test]
    fn a_value_of_the_wrong_shape_says_what_was_expected() {
        let argv = serde_json::from_str::<Argv>("5").unwrap_err().to_string();
        assert!(argv.contains("a program and its arguments, as a list of strings"), "{argv}");
        let host = serde_json::from_str::<HostArgv>("5").unwrap_err().to_string();
        assert!(host.contains("one argv per host (`default`, `linux`, `macos`, `windows`)"), "{host}");
    }

    #[test]
    fn an_argv_reads_as_a_list() {
        let argv: Argv = serde_json::from_str(r#"["docker", "compose", "--project-name", "${run.name}"]"#).unwrap();
        assert_eq!(argv.program().as_str(), "docker");
        assert_eq!(argv.args().len(), 3);
        assert_eq!(serde_json::to_string(&argv).unwrap(), r#"["docker","compose","--project-name","${run.name}"]"#);
        assert!(serde_json::from_str::<Argv>("[]").unwrap_err().to_string().contains("argv is empty"));
    }

    #[test]
    fn a_per_host_argv_names_a_host() {
        let per: HostArgv = serde_json::from_str(r#"{"windows": ["where", "az"], "default": ["which", "az"]}"#).unwrap();
        assert!(matches!(per, HostArgv::PerHost(PerHostArgv { windows: Some(_), .. })));
        assert!(serde_json::from_str::<HostArgv>("{}").unwrap_err().to_string().contains("names no host"));
        assert!(
            serde_json::from_str::<HostArgv>(r#"["sh", "-c", "x"]"#)
                .unwrap_err()
                .to_string()
                .contains("shell")
        );
    }
}
