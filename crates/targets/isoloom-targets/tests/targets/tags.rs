//! The spec's `tags`: on every cloud's resources in the form the cloud takes, and on every
//! Kubernetes object as labels.

use isoloom_spec::Target;
use isoloom_targets::Lab;

use isoloom_test_support::{assert_every, hcl, hcl_blocks, hcl_body, hcl_map, hcl_nested, hcl_one, hcl_values};

use crate::testing::{Outcome, examples, generated as file, lab, problems, valid};

/// segmented with a cost centre and a team.
fn tagged() -> Outcome<Lab> {
    let text = std::fs::read_to_string(examples().join("segmented/isoloom.yml"))?;
    valid(&format!("{text}\ntags:\n  Cost.Center: \"R&D 42\"\n  team: platform\n"))
}

/// A module's tags: its `locals` `tags` (Azure, Oracle Cloud), else AWS's provider's
/// `default_tags`, as (name, value) pairs.
fn module_tags(body: &hcl::Body) -> Vec<(String, String)> {
    let locals = hcl_blocks(body, "locals", &[]).into_iter().flat_map(|l| hcl_map(l.body(), "tags"));
    let aws = hcl_blocks(body, "provider", &["aws"])
        .into_iter()
        .flat_map(|p| hcl_nested(p.body(), "default_tags"))
        .flat_map(|d| hcl_map(d.body(), "tags"));
    locals.chain(aws).collect()
}

/// `pairs` as owned (name, value) pairs.
fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
}

#[test]
fn every_cloud_tags_its_resources_with_the_specs_tags() -> Outcome {
    let lab = tagged()?;
    for cloud in ["aws", "azure", "oci"] {
        let tags = module_tags(&hcl_body(&file(&lab, Target::CloudVm, &format!("cloud-vm/{cloud}/main.tf"))?));
        // The spec's, and Isoloom's own beside them.
        for tag in owned(&[("Cost.Center", "R&D 42"), ("team", "platform"), ("managed-by", "isoloom")]) {
            assert!(tags.contains(&tag), "{cloud}: {tag:?} in {tags:?}");
        }
    }
    // Google Cloud's labels take lowercase letters, digits, `_` and `-`.
    let gcp = hcl_body(&file(&lab, Target::CloudVm, "cloud-vm/gcp/main.tf")?);
    let labels = hcl_map(hcl_one(&gcp, "google_project", "env"), "labels");
    assert!(labels.contains(&owned(&[("cost_center", "r_d_42")])[0]), "{labels:?}");
    assert!(!labels.iter().any(|(k, _)| k == "Cost.Center"), "{labels:?}");
    // DigitalOcean and Linode take tags as `name:value` words.
    for cloud in ["digitalocean", "linode"] {
        let tf = hcl_body(&file(&lab, Target::CloudDocker, &format!("cloud-docker/{cloud}/main.tf"))?);
        let tags = hcl_values(&tf, "tags");
        assert!(
            tags.contains(&"Cost_Center:R_D_42".to_owned()) && tags.contains(&"team:platform".to_owned()),
            "{cloud}: {tags:?}"
        );
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
    let vm = hcl_body(&file(&lab, Target::CloudVm, "cloud-vm/gcp/main.tf")?);
    let docker = hcl_body(&file(&tagged()?, Target::CloudDocker, "cloud-docker/gcp/main.tf")?);
    let team = owned(&[("team", "platform")]).remove(0);
    for (module, body) in [("cloud-vm", &vm), ("cloud-docker", &docker)] {
        let disks: Vec<Vec<(String, String)>> = hcl_blocks(body, "resource", &["google_compute_instance"])
            .iter()
            .flat_map(|i| hcl_nested(i.body(), "boot_disk"))
            .flat_map(|d| hcl_nested(d.body(), "initialize_params"))
            .map(|p| hcl_map(p.body(), "labels"))
            .collect();
        assert_every!(&disks, |labels: &&Vec<(String, String)>| labels.contains(&team));
        assert!(!disks.is_empty(), "{module}");
    }
    assert!(hcl_map(hcl_one(&vm, "google_dns_managed_zone", "private"), "labels").contains(&team));
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
