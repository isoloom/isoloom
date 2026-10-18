//! The daemons an entry runs (design §3.4): for each, its unit and package per family, its
//! foreground form for containers, its user and its log files. The catalogue's daemon table is
//! every entry's daemons plus `catalogue/daemons.yml`, for the daemons no entry owns.

use std::borrow::Cow;

use indexmap::IndexMap;
use isoloom_spec::syntax::{AbsPath, Argv, PackageName, UnitName, UserName};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer};

use crate::family::Family;

/// One daemon.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Daemon {
    /// Its unit: one name on every Linux family, or a name per family (`{ debian: ssh, redhat:
    /// sshd, alpine: sshd }`), which also says the families it exists on.
    pub unit: PerFamily<UnitName>,
    /// The package that ships it, one name or per family: a recipe that installs it without a
    /// `service` step for the daemon gets a warning.
    #[serde(default)]
    pub package: Option<PerFamily<PackageName>>,
    /// Its foreground form: what a container's runner `exec`s (`[nginx, -g, "daemon off;"]`),
    /// one on every Linux family or one per family.
    pub foreground: PerFamily<Argv>,
    /// The user it drops to in a container (default root).
    #[serde(default)]
    pub user: Option<UserName>,
    /// Its log files, each linked to the container's `stdout` or `stderr`.
    #[serde(default)]
    pub logs: IndexMap<AbsPath, LogStream>,
}

/// A container's output stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum LogStream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// A value the same on every Linux family, or given per family.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PerFamily<T> {
    /// The same on Debian's, Red Hat's and Alpine's families.
    Linux(T),
    /// One per family it exists on.
    Each(IndexMap<Family, T>),
}

impl<T> PerFamily<T> {
    /// The value on `family`, when it has one there.
    #[must_use]
    pub fn on(&self, family: Family) -> Option<&T> {
        match self {
            PerFamily::Linux(v) => (family != Family::Windows).then_some(v),
            PerFamily::Each(map) => map.get(&family),
        }
    }

    /// The families it has a value on.
    #[must_use]
    pub fn families(&self) -> Vec<Family> {
        Family::ALL.into_iter().filter(|f| self.on(*f).is_some()).collect()
    }
}

impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for PerFamily<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<PerFamily<T>, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        if value.is_object() {
            serde_json::from_value(value).map(PerFamily::Each).map_err(serde::de::Error::custom)
        } else {
            serde_json::from_value(value).map(PerFamily::Linux).map_err(serde::de::Error::custom)
        }
    }
}

impl<T: JsonSchema> JsonSchema for PerFamily<T> {
    fn schema_name() -> Cow<'static, str> {
        format!("PerFamily_{}", T::schema_name()).into()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        let one = g.subschema_for::<T>();
        json_schema!({
            "anyOf": [
                one,
                {
                    "type": "object",
                    "description": "Per family: `debian` (Debian, Ubuntu, Kali), `redhat` (Rocky, AlmaLinux, Fedora, CentOS), `alpine`, `windows`.",
                    "propertyNames": { "enum": ["debian", "redhat", "alpine", "windows"] },
                    "additionalProperties": one
                }
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_is_one_name_on_linux_or_one_per_family() {
        let d: Daemon =
            serde_yaml_ng::from_str("{ unit: { debian: ssh, redhat: sshd }, foreground: [/usr/sbin/sshd, -D, -e], logs: { /var/log/x.log: stderr } }").unwrap();
        assert_eq!(d.unit.on(Family::RedHat).map(UnitName::as_str), Some("sshd"));
        assert_eq!(d.unit.families(), [Family::Debian, Family::RedHat]);
        let linux: Daemon = serde_yaml_ng::from_str("{ unit: nginx, foreground: [nginx] }").unwrap();
        assert_eq!(linux.unit.families(), [Family::Debian, Family::RedHat, Family::Alpine]);
        assert!(serde_yaml_ng::from_str::<Daemon>("{ unit: { gentoo: x }, foreground: [x] }").is_err());
        let per: Daemon =
            serde_yaml_ng::from_str("{ unit: { debian: cron, redhat: crond }, foreground: { debian: [cron, -f], redhat: [crond, -n] } }").unwrap();
        assert_eq!(per.foreground.on(Family::RedHat).map(Argv::program), Some("crond"));
    }
}
