//! What Isoloom creates says so: every Kubernetes object carries `app.kubernetes.io/managed-by:
//! isoloom` (as Compose's containers carry `isoloom.managed` and the clouds' resources
//! `managed-by`), and the cloud resources that take labels or tags carry the environment's: Google
//! Cloud's project of its own, Linode's firewalls.

use isoloom_spec::Target;
use isoloom_targets::generate;

use isoloom_test_support::{assert_every, hcl, hcl_blocks, hcl_body, hcl_map, hcl_one, hcl_values, yaml_at, yaml_documents};

use crate::testing::{Outcome, example, file};

#[test]
fn every_kubernetes_object_is_labelled_as_isoloom_s() -> Outcome {
    let files = generate(&example("hello-stack")?, Target::Kubernetes)?;
    let env = file(&files, "kubernetes/environment.yaml")?;
    let objects = yaml_documents(env);
    assert!(objects.len() > 3, "{env}");
    assert_every!(&objects, |o: &&serde_yaml_ng::Value| {
        yaml_at(o, "metadata.labels.`app.kubernetes.io/managed-by`").and_then(serde_yaml_ng::Value::as_str) == Some("isoloom")
    });
    // The chart's templates (Helm's, not YAML until rendered): each that makes an object labels it.
    let templates: Vec<&str> = files
        .iter()
        .filter(|f| f.path.contains("kubernetes/chart/templates/") && f.contents.lines().any(|l| l.starts_with("kind: ")))
        .map(|f| f.contents.as_str())
        .collect();
    assert_every!(&templates, |t: &&str| t.lines().any(|l| l.trim() == "app.kubernetes.io/managed-by: isoloom"));
    Ok(())
}

#[test]
fn google_clouds_project_and_linodes_firewalls_carry_the_environments_labels() -> Outcome {
    let lab = example("solo-web")?;
    let vm = generate(&lab, Target::CloudVm)?;
    let docker = generate(&lab, Target::CloudDocker)?;
    for gcp in [file(&vm, "cloud-vm/gcp/main.tf")?, file(&docker, "cloud-docker/gcp/main.tf")?] {
        let body = hcl_body(gcp);
        let labels = hcl_map(hcl_one(&body, "google_project", "env"), "labels");
        for label in [("isoloom-environment", "solo-web"), ("managed-by", "isoloom")] {
            assert!(labels.contains(&(label.0.to_owned(), label.1.to_owned())), "{label:?}: {labels:?}");
        }
    }
    for linode in [file(&vm, "cloud-vm/linode/main.tf")?, file(&docker, "cloud-docker/linode/main.tf")?] {
        let body = hcl_body(linode);
        let firewalls = hcl_blocks(&body, "resource", &["linode_firewall"]);
        assert_every!(&firewalls, |f: &&&hcl::Block| hcl_values(f.body(), "tags").contains(&"isoloom".to_owned()));
    }
    Ok(())
}
