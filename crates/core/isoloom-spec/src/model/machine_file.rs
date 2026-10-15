//! A file placed at a path on a machine (`machines.*.files`): a configuration the image doesn't
//! carry, written the same way on every target.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The mode a file gets when the spec gives none: readable by everyone, written by its owner.
pub const DEFAULT_FILE_MODE: &str = "0644";

/// A file placed on the machine before its workload runs: a container gets it from its runtime
/// (a Compose config, a Kubernetes ConfigMap), a VM from its set-up. On Compose, a project file
/// with an owner or a mode is read into Compose's environment by `isoloom run` and copied in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineFile {
    /// Where the file goes in the machine: an absolute path (`/etc/app/app.ini`). Its folders
    /// are created when missing.
    pub path: String,
    /// The file to copy, from the project folder (`config/app.ini`). Give this or `content`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The file's text, written in the spec. Give this or `source`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// The user id that owns the file (default 0, root). Linux machines only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<u32>,
    /// The group id that owns the file (default 0, root). Linux machines only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gid: Option<u32>,
    /// The file's permissions, in octal (`0600`, `0755`; default `0644`), quoted or not: the
    /// digits are kept as written. Linux machines only.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "octal")]
    #[schemars(with = "Option<super::ScalarSchema>")]
    pub mode: Option<String>,
}

impl MachineFile {
    /// Its mode, the default when the spec gives none.
    pub fn mode_or_default(&self) -> &str {
        self.mode.as_deref().unwrap_or(DEFAULT_FILE_MODE)
    }

    /// Whether the spec sets its owner or its mode.
    pub fn sets_owner_or_mode(&self) -> bool {
        self.uid.is_some() || self.gid.is_some() || self.mode.is_some()
    }
}

/// A mode written as text or as a number, kept as its digits.
fn octal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let raw: Option<serde_json::Value> = Deserialize::deserialize(d)?;
    match raw {
        None => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s)),
        Some(serde_json::Value::Number(n)) => Ok(Some(n.to_string())),
        Some(_) => Err(serde::de::Error::custom("`mode`: octal digits, like \"0644\"")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mode_reads_as_text_or_number() {
        let f: MachineFile = serde_yaml_ng::from_str("{ path: /a, content: x, mode: 0600 }").unwrap();
        assert_eq!(f.mode.as_deref(), Some("0600"));
        let v: serde_yaml_ng::Value = serde_yaml_ng::from_str("{ path: /a, content: x, mode: 0600 }").unwrap();
        let f: MachineFile = serde_yaml_ng::from_value(v).unwrap();
        assert_eq!(f.mode.as_deref(), Some("0600"));
        let f: MachineFile = serde_yaml_ng::from_str("{ path: /a, content: x, mode: 600 }").unwrap();
        assert_eq!(f.mode.as_deref(), Some("600"));
        let f: MachineFile = serde_yaml_ng::from_str("{ path: /a, content: x, mode: \"0600\" }").unwrap();
        assert_eq!(f.mode_or_default(), "0600");
        let f: MachineFile = serde_yaml_ng::from_str("{ path: /a, content: x }").unwrap();
        assert_eq!((f.mode_or_default(), f.sets_owner_or_mode()), ("0644", false));
        assert!(serde_yaml_ng::from_str::<MachineFile>("{ path: /a, mode: [1] }").is_err());
    }
}
