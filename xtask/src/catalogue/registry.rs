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
    #[cfg(unix)]
    use super::super::fetch::testing::Web;
    use super::*;

    #[test]
    fn a_reference_names_its_repository_and_tag_on_docker_hub() {
        let split = |r: &str| split(r).map_err(|e| e.to_string());
        assert_eq!(
            split("docker.io/library/nginx:1.30.6-trixie").unwrap(),
            ("library/nginx".into(), "1.30.6-trixie".into())
        );
        assert_eq!(split("nginx:1.30").unwrap(), ("library/nginx".into(), "1.30".into()));
        assert_eq!(split("example/web-app:v17").unwrap(), ("example/web-app".into(), "v17".into()));
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

    #[cfg(unix)]
    const HUB: &str = "https://registry-1.docker.io/v2/library/nginx";

    /// Docker Hub with an anonymous token for `library/nginx`, and the amd64 image's
    /// configuration (`architecture` `arch`) under `sha256:cfg`.
    #[cfg(unix)]
    fn hub(arch: &str) -> Web {
        let web = Web::new();
        let config = format!(r#"{{"architecture":"{arch}","config":{{"Entrypoint":["/docker-entrypoint.sh"],"Cmd":["nginx","-g","daemon off;"]}}}}"#);
        web.serves(
            "https://auth.docker.io/token?service=registry.docker.io&scope=repository:library/nginx:pull",
            &[],
            br#"{"token":"tok"}"#,
        )
        .serves(&format!("{HUB}/blobs/sha256:cfg"), &[], config.as_bytes())
        .serves(
            &format!("{HUB}/manifests/sha256:amd"),
            &["Docker-Content-Digest: sha256:amd"],
            br#"{"config":{"digest":"sha256:cfg"}}"#,
        );
        web
    }

    #[cfg(unix)]
    #[test]
    fn a_multi_platform_tag_pins_its_index_digest_and_the_amd64_images_process() {
        let web = hub("amd64");
        let index = r#"{"manifests":[{"digest":"sha256:arm","platform":{"os":"linux","architecture":"arm64"}},{"digest":"sha256:amd","platform":{"os":"linux","architecture":"amd64"}}]}"#;
        web.serves(&format!("{HUB}/manifests/1.30"), &["Docker-Content-Digest: sha256:index"], index.as_bytes());
        let pinned = web.browse(|| pin("docker.io/library/nginx:1.30")).unwrap();
        assert_eq!(
            pinned,
            Pinned {
                digest: "sha256:index".into(),
                platforms: vec!["arm64".into(), "amd64".into()],
                entrypoint: vec!["/docker-entrypoint.sh".into()],
                command: vec!["nginx".into(), "-g".into(), "daemon off;".into()],
            }
        );
        // Every registry call carries the token.
        let registry: Vec<Vec<String>> = web.calls().into_iter().filter(|c| c.iter().any(|w| w.starts_with(HUB))).collect();
        assert_eq!(registry.len(), 3, "{registry:?}");
        isoloom_test_support::assert_every!(&registry, |c: &Vec<String>| c.windows(2).any(|w| w[0] == "Bearer" && w[1] == "tok"));
    }

    #[cfg(unix)]
    #[test]
    fn a_single_image_tag_takes_its_platform_from_its_configuration() {
        let web = hub("arm64");
        web.serves(
            &format!("{HUB}/manifests/1.30"),
            &["Docker-Content-Digest: sha256:one"],
            br#"{"config":{"digest":"sha256:cfg"}}"#,
        );
        let pinned = web.browse(|| pin("nginx:1.30")).unwrap();
        assert_eq!((pinned.digest.as_str(), pinned.platforms), ("sha256:one", vec!["arm64".to_owned()]));
    }

    #[cfg(unix)]
    #[test]
    fn a_tag_without_an_amd64_image_or_a_digest_is_refused() {
        let web = hub("amd64");
        let arm_only = r#"{"manifests":[{"digest":"sha256:arm","platform":{"os":"linux","architecture":"arm64"}}]}"#;
        web.serves(&format!("{HUB}/manifests/arm"), &["Docker-Content-Digest: sha256:x"], arm_only.as_bytes())
            .serves(&format!("{HUB}/manifests/nodigest"), &[], br#"{"config":{"digest":"sha256:cfg"}}"#)
            .serves(&format!("{HUB}/manifests/noconfig"), &["Docker-Content-Digest: sha256:y"], b"{}");
        let refused = |r: &str| web.browse(|| pin(r)).unwrap_err().to_string();
        assert_eq!(refused("nginx:arm"), "nginx:arm publishes no amd64 image");
        assert_eq!(refused("nginx:nodigest"), format!("{HUB}/manifests/nodigest: no digest in the answer"));
        assert_eq!(refused("nginx:noconfig"), "nginx:noconfig: no configuration in its manifest");
    }
}
