//! Pinning an official image on Docker Hub, through the registry API with an anonymous token:
//! its tag's multi-platform digest, the platforms it publishes, and its own entrypoint and
//! command (from the amd64 image's configuration), for `lock.yml`.

use serde_json::Value;

use super::fetch;
use crate::Res;

/// What the lock records of an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pinned {
    /// The digest of what the tag names (an index, for a multi-platform image).
    pub digest: String,
    /// The platforms it publishes (`amd64`, `arm64`).
    pub platforms: Vec<String>,
    /// Its own entrypoint.
    pub entrypoint: Vec<String>,
    /// Its own command.
    pub command: Vec<String>,
}

/// The kinds of manifests asked for: an index (OCI or Docker's list), or one image's.
const ACCEPT: &str = "Accept: application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json";

/// Image `reference` (`docker.io/library/nginx:1.30.6-trixie`), pinned.
pub(crate) fn pin(reference: &str) -> Res<Pinned> {
    let (repository, tag) = split(reference)?;
    let token = token(&repository)?;
    let auth = format!("Authorization: Bearer {token}");
    let manifest = |r: &str| -> Res<(String, Value)> {
        let url = format!("https://registry-1.docker.io/v2/{repository}/manifests/{r}");
        let (head, body) = fetch::with_headers(&url, &[auth.clone(), ACCEPT.to_owned()])?;
        let digest = fetch::header(&head, "docker-content-digest")
            .ok_or_else(|| format!("{url}: no digest in the answer"))?
            .to_owned();
        Ok((digest, serde_json::from_slice(&body)?))
    };
    let (digest, top) = manifest(&tag)?;
    let (platforms, image) = match top.get("manifests").and_then(Value::as_array) {
        Some(list) => {
            let platforms = platforms(list);
            let amd64 = list
                .iter()
                .find(|m| m.pointer("/platform/architecture").and_then(Value::as_str) == Some("amd64"))
                .and_then(|m| m.get("digest")?.as_str())
                .ok_or_else(|| format!("{reference} publishes no amd64 image"))?;
            (platforms, manifest(amd64)?.1)
        }
        None => (Vec::new(), top),
    };
    let config_digest = image
        .pointer("/config/digest")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{reference}: no configuration in its manifest"))?;
    let config: Value = serde_json::from_slice(&fetch::get(
        &format!("https://registry-1.docker.io/v2/{repository}/blobs/{config_digest}"),
        &[auth],
    )?)?;
    let strings = |pointer: &str| -> Vec<String> {
        config
            .pointer(pointer)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let platforms = if platforms.is_empty() {
        config.get("architecture").and_then(Value::as_str).map(str::to_owned).into_iter().collect()
    } else {
        platforms
    };
    Ok(Pinned {
        digest,
        platforms,
        entrypoint: strings("/config/Entrypoint"),
        command: strings("/config/Cmd"),
    })
}

/// The Linux platforms an index lists that Isoloom runs (`amd64`, `arm64`), once each.
fn platforms(list: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in list {
        let os = m.pointer("/platform/os").and_then(Value::as_str);
        let arch = m.pointer("/platform/architecture").and_then(Value::as_str);
        if let (Some("linux"), Some(a @ ("amd64" | "arm64"))) = (os, arch)
            && !out.iter().any(|o| o == a)
        {
            out.push(a.to_owned());
        }
    }
    out
}

/// A Docker Hub reference's repository (`library/nginx`) and tag.
fn split(reference: &str) -> Res<(String, String)> {
    let name = reference.strip_prefix("docker.io/").unwrap_or(reference);
    let first = name.split('/').next().unwrap_or_default();
    if name.contains('/') && (first.contains('.') || first.contains(':')) {
        return Err(format!("{reference}: the refresh pins Docker Hub images only").into());
    }
    let (repository, tag) = name
        .rsplit_once(':')
        .filter(|(_, t)| !t.contains('/'))
        .ok_or_else(|| format!("{reference}: give the image's tag"))?;
    let repository = if repository.contains('/') {
        repository.to_owned()
    } else {
        format!("library/{repository}")
    };
    Ok((repository, tag.to_owned()))
}

/// An anonymous pull token for `repository`.
fn token(repository: &str) -> Res<String> {
    let url = format!("https://auth.docker.io/token?service=registry.docker.io&scope=repository:{repository}:pull");
    let answer: Value = serde_json::from_slice(&fetch::get(&url, &[])?)?;
    answer
        .get("token")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{url}: no token in the answer").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_names_its_repository_and_tag_on_docker_hub() {
        let split = |r: &str| split(r).map_err(|e| e.to_string());
        assert_eq!(
            split("docker.io/library/nginx:1.30.6-trixie").unwrap(),
            ("library/nginx".into(), "1.30.6-trixie".into())
        );
        assert_eq!(split("nginx:1.30").unwrap(), ("library/nginx".into(), "1.30".into()));
        assert_eq!(split("bkimminich/juice-shop:v17").unwrap(), ("bkimminich/juice-shop".into(), "v17".into()));
        assert!(split("ghcr.io/x/y:1").unwrap_err().contains("Docker Hub images only"));
        assert!(split("docker.io/library/nginx").unwrap_err().contains("give the image's tag"));
    }

    #[test]
    fn an_index_lists_its_linux_platforms_once() {
        let list: Vec<Value> = serde_json::from_str(
            r#"[{"platform":{"os":"linux","architecture":"amd64"}},{"platform":{"os":"linux","architecture":"arm","variant":"v7"}},{"platform":{"os":"linux","architecture":"arm64"}},{"platform":{"os":"unknown","architecture":"unknown"}},{"platform":{"os":"linux","architecture":"amd64"}}]"#,
        )
        .unwrap();
        assert_eq!(platforms(&list), ["amd64", "arm64"]);
    }
}
