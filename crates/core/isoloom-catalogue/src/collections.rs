//! The official Galaxy collections the Ansible renderer may name, pinned:
//! `catalogue/collections.yml` (design §4.7). They are GPL-3.0, so they are referenced, never
//! embedded: the generated project's requirements name each with its version, its
//! archive's SHA-256 and download address, checked on the controller before `ansible-galaxy`
//! installs it. Only Ansible's own
//! namespace is allowed (`ansible.posix`, `ansible.windows`): no vendor's, no `community.*`.

use isoloom_spec::syntax::{HttpsUrl, Sha256};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The namespace every pinned collection is in.
pub(crate) const NAMESPACE: &str = "ansible";

/// `collections.yml`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Collections {
    /// The pinned collections.
    #[serde(default)]
    pub collections: Vec<Collection>,
}

/// A Galaxy collection, pinned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Collection {
    /// `<namespace>.<name>` (`ansible.posix`).
    pub name: String,
    /// The exact release.
    pub version: String,
    /// Its archive's address on Galaxy.
    pub url: HttpsUrl,
    /// The SHA-256 of its archive, as Galaxy publishes it.
    pub sha256: Sha256,
    /// Its licence, as an SPDX expression.
    pub licence: String,
}

impl Collections {
    /// The collection named `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Collection> {
        self.collections.iter().find(|c| c.name == name)
    }
}
