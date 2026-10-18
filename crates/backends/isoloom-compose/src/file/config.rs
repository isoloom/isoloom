//! The configs of the Compose file (the machines' files), and where a service has them.

use serde::Serialize;

/// A config: a file of the project (`file`, mounted as it is), text (`content`) or a variable of
/// Compose's own environment (`environment`), the last two copied in with the owner and mode
/// its mount gives.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct Config {
    /// A file, from the Compose file's folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) file: Option<String>,
    /// The text (`$` written `$$`: no interpolation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) content: Option<String>,
    /// The variable of Compose's environment holding the text: a project file, read into it by
    /// whoever starts Compose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<String>,
}

/// Where a service has a config, its owner and its mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct ConfigMount {
    /// The config.
    pub(crate) source: String,
    /// The path in the container.
    pub(crate) target: String,
    /// The owner's user id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) uid: Option<String>,
    /// The owner's group id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) gid: Option<String>,
    /// The mode, in octal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mode: Option<String>,
}
