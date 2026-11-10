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
    // A container with set-up scripts, so the chart has its scripts' ConfigMap too.
    let text = std::fs::read_to_string(examples().join("segmented/isoloom.yml"))?;
    let scripted = text.replace("      steps: [{ run: provision/cache.sh }]", "    setup: [provision/cache.sh]");
    let lab = valid(&format!("{scripted}\ntags:\n  Cost.Center: \"R&D 42\"\n"))?;
    let files = isoloom_targets::generate(&lab, Target::Kubernetes)?;
    let mut seen = Vec::new();
    for f in files.iter().filter(|f| std::path::Path::new(&f.path).extension().is_some_and(|e| e == "yaml")) {
        let objects = f
            .contents
            .split("\n---\n")
            .filter(|d| d.contains("\nkind: ") || d.starts_with("kind: "))
            .count();
        if f.path.ends_with("kustomization.yaml") {
            // Its generated ConfigMaps, labelled like every other object.
            let generated = f.contents.contains("configMapGenerator:");
            assert_eq!(f.contents.contains("    cost_center: r_d_42"), generated, "{}: {}", f.path, f.contents);
        } else {
            assert_eq!(f.contents.matches("cost_center: r_d_42").count(), objects, "{}: {}", f.path, f.contents);
        }
        seen.push(f.path.trim_start_matches(".isoloom/kubernetes/").to_owned());
    }
    for path in [
        "checks/job.yaml",
        "chart/templates/scripts.yaml",
        "chart/templates/tests/checks.yaml",
        "checks/kustomization.yaml",
    ] {
        assert!(seen.iter().any(|s| s == path), "{path}: {seen:?}");
    }
    Ok(())
}

#[test]
fn google_cloud_labels_its_boot_disks_and_its_private_zone() -> Outcome {
    let text = std::fs::read_to_string(examples().join("segmented/isoloom.yml"))?;
    let lab = valid(&format!("{text}\ndns: {{ private: true }}\ntags:\n  team: platform\n"))?;
    let vm = file(&lab, Target::CloudVm, "cloud-vm/gcp/main.tf")?;
    let docker = file(&tagged()?, Target::CloudDocker, "cloud-docker/gcp/main.tf")?;
    for tf in [vm.as_str(), docker.as_str()] {
        assert!(tf.contains("initialize_params {"), "{tf}");
        for disk in tf.split("initialize_params {").skip(1) {
            let params = disk.split('}').next().unwrap_or_default();
            assert!(params.contains("labels") && disk.contains("\"platform\""), "{params}");
        }
    }
    let zone = vm.split("resource \"google_dns_managed_zone\"").nth(1).ok_or("no private zone")?;
    assert!(zone.split("\nresource ").next().unwrap_or_default().contains("\"platform\""), "{zone}");
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

#[test]
fn tags_that_one_cloud_would_merge_or_refuse_are_refused() {
    let found = problems(
        &lab("version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\ntags:\n  Cost.Center: a\n  cost_center: b\n  Managed-By: me\n  9lives: x\n")
            .unwrap(),
    );
    for (at, says) in [
        (
            "tags.cost_center",
            "`Cost.Center` and `cost_center` are one label, `cost_center`, on Google Cloud and Kubernetes",
        ),
        ("tags.Managed-By", "`Managed-By` is Isoloom's own tag"),
        ("tags.9lives", "starting with a letter"),
    ] {
        assert!(found.iter().any(|p| p.starts_with(at) && p.contains(says)), "{at}: {found:?}");
    }
    let cased = problems(
        &lab("version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\ntags: { Team: a, team: b }\n").unwrap(),
    );
    assert!(
        cased
            .iter()
            .any(|p| p.starts_with("tags.team") && p.contains("Azure, whose tag names ignore case")),
        "{cased:?}"
    );
}

#[test]
fn a_tag_written_as_a_number_or_a_flag_is_its_text() -> Outcome {
    let numbered = valid(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\ntags: { cost-center: 1234, billable: true }\n",
    )?;
    let tags = numbered.spec().tags;
    assert_eq!((tags["cost-center"].as_str(), tags["billable"].as_str()), ("1234", "true"));
    let refused = Lab::parse("version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\ntags: { team: [a, b] }\n")
        .expect_err("a list is no tag");
    assert!(refused.iter().any(|p| p.path.as_str() == "tags.team"), "{refused:?}");
    Ok(())
}
