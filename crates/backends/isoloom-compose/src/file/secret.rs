//! The secrets of the Compose file, and where a service gets them.

use serde::Serialize;

/// A secret of the file: where Compose reads its value when it creates the containers.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Secret {
    /// The variable of Compose's own environment holding the value (never the containers').
    pub(crate) environment: String,
}

/// A secret a service gets: which one, at what path, owned by whom, with what mode.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ServiceSecret {
    /// The file's secret.
    pub(crate) source: String,
    /// The file's absolute path in the container.
    pub(crate) target: String,
    /// Its owner's user id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) uid: Option<String>,
    /// Its group id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) gid: Option<String>,
    /// Its mode, in octal (`"0440"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mode: Option<String>,
}
