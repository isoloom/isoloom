//! What Isoloom creates says so: every Kubernetes object carries `app.kubernetes.io/managed-by:
//! isoloom` (as Compose's containers carry `isoloom.managed` and the clouds' resources
//! `managed-by`), and the cloud resources that take labels or tags carry the environment's: Google
//! Cloud's project of its own, Linode's firewalls.

use isoloom_spec::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, example, file};

#[test]
fn every_kubernetes_object_is_labelled_as_isoloom_s() -> Outcome {
    let files = generate(&example("hello-stack")?, Target::Kubernetes)?;
    let env = file(&files, "kubernetes/environment.yaml")?;
    let objects: Vec<&str> = env.split("\n---\n").filter(|d| d.contains("\nkind: ")).collect();
    assert!(objects.len() > 3, "{env}");
    for o in objects {
        let metadata = o.split("\nmetadata:\n").nth(1).ok_or("no metadata")?;
        let own = metadata.lines().take_while(|l| l.starts_with("  ")).collect::<Vec<_>>().join("\n");
        assert!(own.contains("    app.kubernetes.io/managed-by: isoloom"), "{o}");
    }
    let chart: String = files
        .iter()
        .filter(|f| f.path.contains("kubernetes/chart/templates/"))
        .map(|f| f.contents.as_str())
        .collect();
    assert!(chart.contains("app.kubernetes.io/managed-by: isoloom"), "the chart's objects too");
    Ok(())
}

#[test]
fn google_clouds_project_and_linodes_firewalls_carry_the_environments_labels() -> Outcome {
    let lab = example("solo-web")?;
    let vm = generate(&lab, Target::CloudVm)?;
    let docker = generate(&lab, Target::CloudDocker)?;
    for gcp in [file(&vm, "cloud-vm/gcp/main.tf")?, file(&docker, "cloud-docker/gcp/main.tf")?] {
        let project = gcp.split("resource \"google_project\" \"env\"").nth(1).ok_or("no project")?;
        let project = project.split("\n}\n").next().unwrap_or_default();
        let labels = project.split("\n  labels = {\n").nth(1).ok_or("no labels on the project")?;
        let labels = labels.split("\n  }\n").next().unwrap_or_default();
        assert!(
            labels.contains("isoloom-environment = \"solo-web\"") && labels.contains("managed-by          = \"isoloom\""),
            "{labels}"
        );
    }
    for linode in [file(&vm, "cloud-vm/linode/main.tf")?, file(&docker, "cloud-docker/linode/main.tf")?] {
        let firewalls: Vec<&str> = linode.split("resource \"linode_firewall\"").skip(1).collect();
        assert!(!firewalls.is_empty(), "{linode}");
        for f in firewalls {
            let f = f.split("\n}\n").next().unwrap_or_default();
            assert!(f.contains("tags ") && f.contains("isoloom"), "{f}");
        }
    }
    Ok(())
}
