//! A machine's `files`: one way to place a file, whatever its source. A project file or inline
//! text is configuration (it may land in generated files); an input or a secret is a run-time
//! value, delivered at each start and never written into a generated file, an image layer or an
//! environment variable.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer};

use super::invalid::Invalid;
use super::names::{InputName, Text};

/// Where a file's contents come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSource {
    /// A file of the project, copied as it is.
    Source(String),
    /// The text written in the spec.
    Content(String),
    /// An input's value, given at each start.
    Input(InputName),
    /// A secret's value, read from its store at each start.
    Secret(String),
}

impl FileSource {
    /// Whether the value is only known at run time (an input or a secret).
    #[must_use]
    pub fn is_value(&self) -> bool {
        matches!(self, FileSource::Input(_) | FileSource::Secret(_))
    }

    /// The field that gives it (`source`, `content`, `input`, `secret`).
    #[must_use]
    pub fn key(&self) -> &'static str {
        match self {
            FileSource::Source(_) => "source",
            FileSource::Content(_) => "content",
            FileSource::Input(_) => "input",
            FileSource::Secret(_) => "secret",
        }
    }
}

/// A file's permissions, octal, at most `0777`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Mode(u32);

impl Mode {
    /// The mode a project file or inline text gets when none is given: everyone reads it.
    pub const CONFIG: Mode = Mode(0o644);
    /// The mode an input's or a secret's file gets when none is given: root reads it.
    pub const VALUE: Mode = Mode(0o400);

    /// The mode as a number (`0o444`).
    #[must_use]
    pub fn bits(self) -> u32 {
        self.0
    }
}

impl FromStr for Mode {
    type Err = Invalid;

    /// The mode `digits` (`"0444"`, or YAML's number `444`) names.
    ///
    /// # Errors
    ///
    /// [`Invalid::Mode`] when they aren't octal digits up to `0777`.
    fn from_str(digits: &str) -> Result<Mode, Invalid> {
        u32::from_str_radix(digits, 8)
            .ok()
            .filter(|m| *m <= 0o777)
            .map(Mode)
            .ok_or_else(|| Invalid::Mode(digits.to_owned()))
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04o}", self.0)
    }
}

impl<'de> Deserialize<'de> for Mode {
    /// Octal digits as text (`"0444"`), or the number YAML reads them as (`444`).
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Mode, D::Error> {
        d.deserialize_any(Text("octal digits, like \"0444\""))?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Mode {
    fn schema_name() -> Cow<'static, str> {
        "Mode".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": ["string", "integer"], "pattern": "^0?[0-7]{1,3}$" })
    }
}

/// A file placed on a machine.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "FileInput")]
pub struct File {
    /// Its absolute path in the machine (a drive path, `C:\…`, on Windows).
    pub path: String,
    /// Where its contents come from.
    pub source: FileSource,
    /// Its owner's user id (default 0). Not on Windows.
    pub uid: Option<u32>,
    /// Its group id (default 0). Not on Windows.
    pub gid: Option<u32>,
    /// Its mode, when the spec gives one (see [`File::mode_or_default`]).
    pub mode: Option<Mode>,
}

impl File {
    /// Its mode: the spec's, else `0644` for configuration and `0400` for a value.
    #[must_use]
    pub fn mode_or_default(&self) -> Mode {
        self.mode.unwrap_or(if self.source.is_value() { Mode::VALUE } else { Mode::CONFIG })
    }
}

/// A file as written.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FileInput {
    /// Where the file goes: an absolute path (`/etc/app/app.ini`), or a drive path (`C:\app\flag.txt`)
    /// on Windows. Its folders are created when missing.
    path: String,
    /// A file of the project to copy (`config/app.ini`). Exactly one of `source`, `content`,
    /// `input` and `secret`.
    #[serde(default)]
    source: Option<String>,
    /// The file's text, written in the spec.
    #[serde(default)]
    content: Option<String>,
    /// An input's name: the file holds its value, given at each start, before the machine's
    /// process starts. Never in a generated file, an image or an environment variable.
    #[serde(default)]
    input: Option<InputName>,
    /// A secret's name: the file holds its value, read from its store at each start.
    #[serde(default)]
    secret: Option<String>,
    /// The user id that owns the file (default 0, root). Not on Windows.
    #[serde(default)]
    uid: Option<u32>,
    /// The group id that owns the file (default 0, root). Not on Windows.
    #[serde(default)]
    gid: Option<u32>,
    /// Its permissions in octal (`"0444"`): default `0644` for `source` and `content`, `0400`
    /// for `input` and `secret` (only root reads it; widen it for an app's own user). Not on
    /// Windows, where a value file is readable by SYSTEM and Administrators.
    #[serde(default)]
    mode: Option<Mode>,
}

impl TryFrom<FileInput> for File {
    type Error = String;

    fn try_from(f: FileInput) -> Result<File, String> {
        let given: Vec<FileSource> = [
            f.source.map(FileSource::Source),
            f.content.map(FileSource::Content),
            f.input.map(FileSource::Input),
            f.secret.map(FileSource::Secret),
        ]
        .into_iter()
        .flatten()
        .collect();
        let source = match <[FileSource; 1]>::try_from(given) {
            Ok([one]) => one,
            Err(given) if given.is_empty() => return Err("a file needs one of `source`, `content`, `input` and `secret`".into()),
            Err(given) => {
                let keys: Vec<&str> = given.iter().map(FileSource::key).collect();
                return Err(format!("`{}` are all given: keep one source", keys.join("` and `")));
            }
        };
        Ok(File {
            path: f.path,
            source,
            uid: f.uid,
            gid: f.gid,
            mode: f.mode,
        })
    }
}

impl JsonSchema for File {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "File".into()
    }

    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        FileInput::json_schema(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_has_one_source_and_a_mode_by_kind() {
        let value: File = serde_yaml_ng::from_str("{ path: /ctf/flag, input: FLAG }").unwrap();
        assert!(value.source.is_value());
        assert_eq!(value.mode_or_default().to_string(), "0400");
        let widened: File = serde_yaml_ng::from_str("{ path: /a, content: x, mode: 0444 }").unwrap();
        assert_eq!(widened.mode_or_default(), Mode(0o444));
        let config: File = serde_yaml_ng::from_str("{ path: /a, source: a.ini }").unwrap();
        assert_eq!(config.mode_or_default(), Mode::CONFIG);
    }

    #[test]
    fn a_file_with_two_sources_or_none_is_refused() {
        let two = serde_yaml_ng::from_str::<File>("{ path: /a, source: a, input: FLAG }").unwrap_err().to_string();
        assert!(two.contains("`source` and `input`"), "{two}");
        let none = serde_yaml_ng::from_str::<File>("{ path: /a }").unwrap_err().to_string();
        assert!(none.contains("needs one of"), "{none}");
        assert!(serde_yaml_ng::from_str::<File>("{ path: /a, content: x, mode: 0999 }").is_err());
    }
}
