//! Every container Isoloom adds to a Compose project (the check runners, the probe, the route
//! sidecars, the switches, the router, the tools) runs an image pinned by digest, as the security
//! page says: on every example, each `isoloom-*` service's image is `<name>:<tag>@sha256:<64 hex>`,
//! behind a registry mirror or not. The `shell` toolbox and the `tc` sidecar once ran
//! `nicolaka/netshoot` by tag, one with no tag at all.

use isoloom_spec::model::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, examples};

/// Whether `image` is `<name>:<tag>@sha256:<64 hex>`.
fn pinned(image: &str) -> bool {
    let Some((named, digest)) = image.split_once("@sha256:") else { return false };
    let tagged = named.rsplit_once(':').is_some_and(|(_, tag)| !tag.is_empty() && !tag.contains('/'));
    tagged && digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit())
}

#[test]
fn every_helper_container_runs_an_image_pinned_by_digest() -> Outcome {
    fn image_of<'a>((name, service): (&'a serde_yaml_ng::Value, &'a serde_yaml_ng::Value)) -> Option<(&'a str, &'a str)> {
        Some((name.as_str()?, service.get("image")?.as_str()?))
    }
    let mut dirs: Vec<_> = std::fs::read_dir(examples())?.map(|e| e.map(|e| e.path())).collect::<Result<_, _>>()?;
    dirs.retain(|d| d.join("isoloom.yml").is_file());
    let mut seen = 0;
    for dir in &dirs {
        let lab = isoloom_targets::Lab::new(isoloom_spec::load(dir)?);
        let Ok(files) = generate(&lab, Target::Docker) else { continue };
        let compose = files.iter().find(|f| f.path == ".isoloom/docker/compose.yml").ok_or("no compose.yml")?;
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&compose.contents)?;
        let services = doc.get("services").and_then(|s| s.as_mapping()).ok_or("no services")?;
        // A machine's own image, which the jobs seeding its volumes run too: the author's choice.
        let machines: Vec<&str> = services
            .iter()
            .filter_map(image_of)
            .filter(|(n, _)| !n.starts_with("isoloom-"))
            .map(|(_, i)| i)
            .collect();
        for (name, image) in services.iter().filter_map(image_of) {
            if name.starts_with("isoloom-") && !machines.contains(&image) {
                seen += 1;
                assert!(pinned(image), "{}: `{name}` runs `{image}`, which isn't pinned by digest", dir.display());
            }
        }
    }
    assert!(seen > 20, "only {seen} helper containers among the examples");
    Ok(())
}

#[test]
fn a_pinned_image_has_a_tag_and_a_digest() {
    assert!(pinned(
        "nicolaka/netshoot:v0.14@sha256:7f08c4aff13ff61a35d30e30c5c1ea8396eac6ab4ce19fd02d5a4b3b5d0d09a2"
    ));
    assert!(pinned(
        "registry.corp/hub/library/alpine:3.24@sha256:f59abe656b692d901e184be3445e084786602108599d2a51d229c0f5a48c71bf"
    ));
    assert!(!pinned("nicolaka/netshoot") && !pinned("nicolaka/netshoot:v0.13"));
    assert!(
        !pinned("nicolaka/netshoot@sha256:7f08c4aff13ff61a35d30e30c5c1ea8396eac6ab4ce19fd02d5a4b3b5d0d09a2"),
        "a digest with no tag"
    );
}
