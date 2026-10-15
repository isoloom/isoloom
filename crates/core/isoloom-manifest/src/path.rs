//! Paths: relative to the manifest's folder (or to the run's state folder, through
//! `${run.state_dir}`), `/`-separated on every host, never absolute and never climbing out with
//! `..`; and the absolute paths inside a machine.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::template::{Part, Placeholder, RunVar, Template};
use crate::{Error, Result};

/// The JSON Schema form of the rules a relative path follows (`[^\u0000]+` checks the rest).
const RELATIVE: &str = "^(?![/\\\\])(?![A-Za-z]:)(?![\\s\\S]*\\\\)(?!(?:[\\s\\S]*/)?\\.\\.(?:/|$))";

/// Checks the rules of a relative path on `text` (placeholders already stood in for): `shown` is
/// what the error shows.
fn check_relative(text: &str, shown: &str) -> Result<()> {
    let refused = |make: fn(String) -> Error| Err(make(shown.to_owned()));
    if text.is_empty() {
        return Err(Error::EmptyPath);
    }
    if text.contains('\\') {
        return refused(|path| Error::Backslash { path });
    }
    let drive = matches!(text.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic());
    if text.starts_with('/') || drive {
        return refused(|path| Error::AbsolutePath { path });
    }
    if text.split('/').any(|segment| segment == "..") {
        return refused(|path| Error::ParentPath { path });
    }
    Ok(())
}

/// A file or folder relative to the manifest's folder (or, where a field says so, to the run's
/// state folder): `/`-separated, never absolute, never leaving its folder.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPath(String);

impl RelPath {
    /// The path, when it follows the rules.
    ///
    /// # Errors
    ///
    /// [`Error::Pattern`] for a NUL character, [`Error::EmptyPath`],
    /// [`Error::Backslash`], [`Error::AbsolutePath`] (a `/` or a drive first) and
    /// [`Error::ParentPath`] (a `..` segment).
    pub fn parse(path: &str) -> Result<RelPath> {
        if path.contains('\0') {
            return Err(Error::Pattern {
                what: "path",
                value: path.escape_default().to_string(),
                rule: "a path holds no NUL character",
            });
        }
        check_relative(path, path)?;
        Ok(RelPath(path.to_owned()))
    }

    /// The path as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for RelPath {
    type Err = Error;

    fn from_str(path: &str) -> Result<RelPath> {
        RelPath::parse(path)
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for RelPath {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RelPath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<RelPath, D::Error> {
        RelPath::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for RelPath {
    fn schema_name() -> Cow<'static, str> {
        "RelPath".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "Relative to the manifest's folder (or, where said, to the run's state folder), `/`-separated, never absolute, never leaving its folder with `..`. The runner converts separators for the host.",
            "type": "string",
            "pattern": format!("{RELATIVE}[^\\u0000]+$"),
        })
    }
}

/// A path that may start at a run folder (`${run.state_dir}/ports.yml`), else relative to the
/// manifest's folder: a [`Template`] following the rules of a [`RelPath`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PathTemplate(Template);

impl PathTemplate {
    /// The path template written as `text`.
    ///
    /// # Errors
    ///
    /// What [`Template::parse`] refuses, then what [`RelPath::parse`] refuses in its literal text
    /// (each placeholder standing for one plain segment).
    pub fn parse(text: &str) -> Result<PathTemplate> {
        let template = Template::parse(text)?;
        // A placeholder stands for one plain segment: only the literal text can break a rule.
        let stood_in: String = template
            .parts()
            .iter()
            .map(|p| match p {
                Part::Text(t) => t.as_str(),
                Part::Placeholder(_) => "x",
            })
            .collect();
        check_relative(&stood_in, text)?;
        Ok(PathTemplate(template))
    }

    /// `path`, relative to the manifest's folder.
    #[must_use]
    pub fn relative(path: &RelPath) -> PathTemplate {
        PathTemplate(Template::text(path.as_str()))
    }

    /// `path` in the run's state folder: `${run.state_dir}/<path>`.
    #[must_use]
    pub fn in_state_dir(path: &RelPath) -> PathTemplate {
        PathTemplate(Template::from_parts([
            Part::Placeholder(Placeholder::Run(RunVar::StateDir)),
            Part::Text(format!("/{path}")),
        ]))
    }

    /// The template.
    #[must_use]
    pub fn template(&self) -> &Template {
        &self.0
    }
}

impl FromStr for PathTemplate {
    type Err = Error;

    fn from_str(text: &str) -> Result<PathTemplate> {
        PathTemplate::parse(text)
    }
}

impl fmt::Display for PathTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for PathTemplate {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de> Deserialize<'de> for PathTemplate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<PathTemplate, D::Error> {
        PathTemplate::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for PathTemplate {
    fn schema_name() -> Cow<'static, str> {
        "PathTemplate".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A path that may start at a run folder (`${run.state_dir}/ports.yml`), else relative to the manifest's folder: a template with the rules of a relative path.",
            "allOf": [generator.subschema_for::<Template>(), { "pattern": format!("{RELATIVE}[\\s\\S]+$") }],
        })
    }
}

/// An absolute path inside a machine (`/ctf`): where a volume is mounted.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GuestPath(String);

impl GuestPath {
    /// The path, when it is absolute.
    ///
    /// # Errors
    ///
    /// [`Error::RelativeGuestPath`] when it doesn't start with `/` or holds a NUL
    /// character.
    pub fn parse(path: &str) -> Result<GuestPath> {
        if path.starts_with('/') && !path.contains('\0') {
            Ok(GuestPath(path.to_owned()))
        } else {
            Err(Error::RelativeGuestPath {
                path: path.escape_default().to_string(),
            })
        }
    }

    /// The path as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for GuestPath {
    type Err = Error;

    fn from_str(path: &str) -> Result<GuestPath> {
        GuestPath::parse(path)
    }
}

impl fmt::Display for GuestPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for GuestPath {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for GuestPath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<GuestPath, D::Error> {
        GuestPath::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for GuestPath {
    fn schema_name() -> Cow<'static, str> {
        "GuestPath".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "description": "An absolute path inside a machine.", "type": "string", "pattern": "^/[^\\u0000]*$" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_stay_inside_their_folder() {
        for ok in ["compose.yml", "checks/exec-web.sh", "./a", "a/b..c", ".hidden"] {
            assert!(RelPath::parse(ok).is_ok(), "{ok}");
        }
        let code = |p: &str| RelPath::parse(p).unwrap_err().code();
        assert_eq!(code(""), "empty_path");
        assert_eq!(code("/etc/passwd"), "absolute_path");
        assert_eq!(code("C:/x"), "absolute_path");
        assert_eq!(code("checks\\a.sh"), "backslash");
        assert_eq!(code("../x"), "parent_path");
        assert_eq!(code("a/../../x"), "parent_path");
    }

    #[test]
    fn a_path_template_starts_at_a_run_folder_or_is_relative() {
        let pinned = PathTemplate::in_state_dir(&RelPath::parse("ports.yml").unwrap());
        assert_eq!(pinned.to_string(), "${run.state_dir}/ports.yml");
        assert_eq!(PathTemplate::parse("${run.state_dir}/ports.yml").unwrap(), pinned);
        assert_eq!(PathTemplate::parse("/tmp/${run.name}").unwrap_err().code(), "absolute_path");
        assert_eq!(PathTemplate::parse("${run.dir}/../x").unwrap_err().code(), "parent_path");
        assert_eq!(PathTemplate::parse("${secrets.x}").unwrap_err().code(), "secret_placeholder");
    }

    #[test]
    fn paths_read_back_as_written() {
        let rel: RelPath = "checks/web.sh".parse().unwrap();
        assert_eq!((rel.as_str(), rel.to_string()), ("checks/web.sh", "checks/web.sh".to_owned()));
        let relative = PathTemplate::relative(&rel);
        assert_eq!(relative.template(), &Template::text("checks/web.sh"));
        assert_eq!("${run.dir}/a".parse::<PathTemplate>().unwrap().template().placeholders().count(), 1);
        let guest: GuestPath = "/ctf".parse().unwrap();
        assert_eq!((guest.as_str(), guest.to_string()), ("/ctf", "/ctf".to_owned()));
    }

    #[test]
    fn a_guest_path_is_absolute() {
        assert!(GuestPath::parse("/ctf").is_ok());
        assert_eq!(GuestPath::parse("/a\0b").unwrap_err().code(), "relative_guest_path");
        assert_eq!(GuestPath::parse("ctf").unwrap_err().code(), "relative_guest_path");
    }
}
