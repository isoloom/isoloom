//! A shared volume as a folder of the Proxmox server (`volumes.<name>.virtiofs`): each VM that
//! mounts the volume gets the folder through virtiofs instead of over NFS from its owner.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The most virtiofs folders one Proxmox VM takes (the bpg provider's limit on `virtiofs`).
pub const MAX_VIRTIOFS: usize = 8;

/// On Proxmox, the volume as a folder of the server, shared into each VM that mounts it
/// (virtiofs, Proxmox VE 8.4 or later) instead of over NFS from its owner. Opt-in, because the
/// folder lives on the server: Isoloom creates a directory mapping for it unless `mapping` names
/// one made on the cluster, and the folder itself must exist on the node. Linux VMs only; every
/// other target shares the volume as it always does.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Virtiofs {
    /// An existing directory mapping of the cluster, by its id (Datacenter, Resource Mappings,
    /// Directory mappings). By default Isoloom creates one, `iso<slot>-<volume>`, for the folder
    /// `<virtiofs_root>/<environment>/<volume>` on the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mapping: Option<String>,
    /// How the guests cache the folder: `auto` (the default: metadata kept a short while, so
    /// another VM's changes show within a second), `always`, `metadata` or `never` (every access
    /// goes to the server: slowest, always current).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<VirtiofsCache>,
    /// Reads and writes that bypass the guest's page cache when a program asks for it
    /// (`O_DIRECT`), for databases that manage their own cache. Off by default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub direct_io: bool,
    /// Pass extended attributes through (SELinux labels, file capabilities). Off by default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub xattr: bool,
    /// Pass POSIX ACLs through (`setfacl`), which implies `xattr`. Off by default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub acl: bool,
}

/// How a VM caches a virtiofs folder (see [`Virtiofs::cache`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VirtiofsCache {
    /// Metadata and lookups kept a short while, or until they change: Proxmox's default.
    #[default]
    Auto,
    /// Everything kept: fastest, but another VM's changes may not show.
    Always,
    /// Metadata kept, data read each time.
    Metadata,
    /// Nothing kept: every access goes to the server.
    Never,
}

impl VirtiofsCache {
    /// The value Proxmox takes.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            VirtiofsCache::Auto => "auto",
            VirtiofsCache::Always => "always",
            VirtiofsCache::Metadata => "metadata",
            VirtiofsCache::Never => "never",
        }
    }
}

impl Virtiofs {
    /// Whether extended attributes pass through: asked for, or implied by ACLs.
    #[must_use]
    pub fn exposes_xattr(&self) -> bool {
        self.xattr || self.acl
    }
}

/// Whether `id` can name a Proxmox directory mapping: a letter, then letters, digits, `-` and
/// `_` (Proxmox's configuration ids), at most 64.
#[must_use]
pub fn mapping_id(id: &str) -> bool {
    id.len() <= 64 && id.starts_with(|c: char| c.is_ascii_alphabetic()) && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cache_goes_by_the_name_proxmox_takes() -> Result<(), serde_yaml_ng::Error> {
        for cache in [VirtiofsCache::Auto, VirtiofsCache::Always, VirtiofsCache::Metadata, VirtiofsCache::Never] {
            assert_eq!(serde_yaml_ng::from_str::<VirtiofsCache>(cache.id())?, cache);
        }
        Ok(())
    }

    #[test]
    fn extended_attributes_pass_when_asked_for_or_acls_need_them() -> Result<(), serde_yaml_ng::Error> {
        let mut read = Vec::new();
        for yaml in ["{}", "{ xattr: true }", "{ acl: true }"] {
            read.push(serde_yaml_ng::from_str::<Virtiofs>(yaml)?.exposes_xattr());
        }
        assert_eq!(read, [false, true, true]);
        Ok(())
    }

    #[test]
    fn a_mapping_id_is_a_letter_then_up_to_63_letters_digits_dashes_or_underscores() {
        let longest = format!("a{}", "b".repeat(63));
        let ids = ["share-1_a", longest.as_str(), &format!("{longest}c"), "1share", "a b", "a.b"];
        assert_eq!(ids.map(mapping_id), [true, true, false, false, false, false]);
    }
}
