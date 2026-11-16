//! Pinning the official Galaxy collections the Ansible renderer may name: each
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

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::super::fetch::testing::Web;
    use super::*;

    #[test]
    fn a_collection_s_highest_version_and_its_release_are_read_from_galaxy() {
        let web = Web::new();
        web.serves(&format!("{INDEX}/ansible/posix/"), &[], br#"{"highest_version":{"version":"2.1.0"}}"#)
            .serves(
                &format!("{INDEX}/ansible/posix/versions/2.1.0/"),
                &[],
                br#"{"download_url":"https://galaxy.example/ansible-posix-2.1.0.tar.gz","artifact":{"sha256":"ab12"}}"#,
            );
        assert_eq!(web.browse(|| highest("ansible.posix")).ok().as_deref(), Some("2.1.0"));
        let release = web.browse(|| release("ansible.posix", "2.1.0")).unwrap();
        assert_eq!(
            release,
            Release {
                version: "2.1.0".into(),
                url: "https://galaxy.example/ansible-posix-2.1.0.tar.gz".into(),
                sha256: "ab12".into(),
            }
        );
    }

    #[test]
    fn a_name_or_an_answer_galaxy_wouldn_t_give_is_refused_naming_it() {
        let web = Web::new();
        web.serves(&format!("{INDEX}/ansible/posix/"), &[], b"{}").serves(
            &format!("{INDEX}/ansible/posix/versions/2.1.0/"),
            &[],
            br#"{"download_url":"https://x"}"#,
        );
        assert_eq!(highest("posix").unwrap_err().to_string(), "`posix` isn't `<namespace>.<name>`");
        assert_eq!(release("posix", "1").unwrap_err().to_string(), "`posix` isn't `<namespace>.<name>`");
        assert_eq!(
            web.browse(|| highest("ansible.posix")).unwrap_err().to_string(),
            "Galaxy lists no version of ansible.posix"
        );
        let e = web.browse(|| release("ansible.posix", "2.1.0")).unwrap_err().to_string();
        assert_eq!(e, "Galaxy's ansible.posix 2.1.0 has no `/artifact/sha256`");
    }
}
