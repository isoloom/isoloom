//! Pinning the official Galaxy collections the Ansible renderer may name (design §4.7): each
//! one's version, its archive's address and SHA-256, from Galaxy's API.

use serde_json::Value;

use super::fetch;
use crate::Res;

/// The collections the renderer names, with their licence: Ansible's own namespace only.
pub(crate) const PINNED: [(&str, &str); 2] = [("ansible.posix", "GPL-3.0-or-later"), ("ansible.windows", "GPL-3.0-or-later")];

/// Galaxy's collection index.
const INDEX: &str = "https://galaxy.ansible.com/api/v3/plugin/ansible/content/published/collections/index";

/// A collection's release on Galaxy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Release {
    pub version: String,
    pub url: String,
    pub sha256: String,
}

/// Collection `name`'s highest version on Galaxy.
pub(crate) fn highest(name: &str) -> Res<String> {
    let (namespace, short) = name.split_once('.').ok_or_else(|| format!("`{name}` isn't `<namespace>.<name>`"))?;
    let answer: Value = serde_json::from_slice(&fetch::get(&format!("{INDEX}/{namespace}/{short}/"), &[])?)?;
    answer
        .pointer("/highest_version/version")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("Galaxy lists no version of {name}").into())
}

/// Collection `name`'s release `version`: its archive's address and SHA-256.
pub(crate) fn release(name: &str, version: &str) -> Res<Release> {
    let (namespace, short) = name.split_once('.').ok_or_else(|| format!("`{name}` isn't `<namespace>.<name>`"))?;
    let answer: Value = serde_json::from_slice(&fetch::get(&format!("{INDEX}/{namespace}/{short}/versions/{version}/"), &[])?)?;
    let field = |p: &str| {
        answer
            .pointer(p)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("Galaxy's {name} {version} has no `{p}`"))
    };
    Ok(Release {
        version: version.to_owned(),
        url: field("/download_url")?,
        sha256: field("/artifact/sha256")?,
    })
}
