//! The spec's `tags`: on every cloud's resources in the form the cloud takes, and on every
//! Kubernetes object as labels.

use isoloom_spec::Target;
use isoloom_targets::Lab;

use crate::testing::{Outcome, examples, generated as file, lab, problems, valid};

/// segmented with a cost centre and a team.
fn tagged() -> Outcome<Lab> {
    let text = std::fs::read_to_string(examples().join("segmented/isoloom.yml"))?;
    valid(&format!("{text}\ntags:\n  Cost.Center: \"R&D 42\"\n  team: platform\n"))
}

#[test]
fn every_cloud_tags_its_resources_with_the_specs_tags() -> Outcome {
    let lab = tagged()?;
    for cloud in ["aws", "azure", "oci"] {
        let tf = file(&lab, Target::CloudVm, &format!("cloud-vm/{cloud}/main.tf"))?;
        assert!(
            tf.contains("\"Cost.Center\"") && tf.contains("\"R&D 42\"") && tf.contains("\"platform\""),
            "{cloud}: {tf}"
        );
        assert!(tf.contains("managed-by"), "Isoloom's own stay: {cloud}");
    }
    let aws = file(&lab, Target::CloudVm, "cloud-vm/aws/main.tf")?;
    let default_tags = aws.split("default_tags").nth(1).ok_or("no default_tags")?;
    assert!(default_tags.contains("\"Cost.Center\"") && default_tags.contains("= \"R&D 42\""), "{aws}");
    // Google Cloud's labels take lowercase letters, digits, `_` and `-`.
    let gcp = file(&lab, Target::CloudVm, "cloud-vm/gcp/main.tf")?;
    assert!(gcp.contains("cost_center") && gcp.contains("= \"r_d_42\""), "{gcp}");
    assert!(!gcp.contains("Cost.Center"), "{gcp}");
    for cloud in ["digitalocean", "linode"] {
        let tf = file(&lab, Target::CloudDocker, &format!("cloud-docker/{cloud}/main.tf"))?;
        assert!(tf.contains("\"Cost_Center:R_D_42\"") && tf.contains("\"team:platform\""), "{cloud}: {tf}");
    }
    Ok(())
}

#[test]
fn every_kubernetes_object_carries_the_tags_as_labels() -> Outcome {
    let env = file(&tagged()?, Target::Kubernetes, "kubernetes/environment.yaml")?;
    let objects = env.split("\n---\n").filter(|d| d.contains("kind: ")).count();
    assert!(objects > 3, "{env}");
    assert_eq!(env.matches("cost_center: r_d_42").count(), objects, "{env}");
    Ok(())
}

#[test]
fn a_tag_is_one_every_cloud_takes_and_not_isoloom_s_own() {
    let found = problems(
        &lab("version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\ntags:\n  \"a/b\": x\n  managed-by: me\n  ok: \"line\\nbreak\"\n")
            .unwrap(),
    );
    for at in ["tags.a/b", "tags.managed-by", "tags.ok"] {
        assert!(found.iter().any(|p| p.starts_with(at)), "{at}: {found:?}");
    }
}
