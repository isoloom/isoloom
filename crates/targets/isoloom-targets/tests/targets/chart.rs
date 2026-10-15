//! The Helm chart beside the Kubernetes manifests: the same environment, images and inputs as
//! values, checks as `helm test`, the project's scripts copied in.

use std::path::PathBuf;

use isoloom_spec::{Target, parse};
use isoloom_targets::{GeneratedFile, Lab, generate, project_copies};

use crate::testing::{Outcome, example, examples, file};

/// The example `name`'s folder and its Kubernetes files.
fn kubernetes(name: &str) -> Outcome<(PathBuf, Vec<GeneratedFile>)> {
    Ok((examples().join(name), generate(&example(name)?, Target::Kubernetes)?))
}

#[test]
fn the_chart_is_the_same_environment_with_images_as_values() -> Outcome {
    let (_, files) = kubernetes("hello-stack")?;
    let chart = file(&files, "chart/Chart.yaml")?;
    assert!(
        chart.contains("apiVersion: v2\nname: hello-stack\n") && chart.contains("version: 0.1.0"),
        "{chart}"
    );
    let values: serde_yaml_ng::Value = serde_yaml_ng::from_str(file(&files, "chart/values.yaml")?).unwrap();
    assert_eq!(values["images"]["cache"].as_str(), Some("redis:7-alpine"));
    assert_eq!(values["images"]["web"].as_str(), Some("isoloom/hello-stack-web"));

    let env = file(&files, "chart/templates/environment.yaml")?;
    assert!(!env.contains("kind: Namespace"), "the release's namespace");
    assert!(env.contains("image: '{{ index .Values.images \"cache\" }}'"));
    assert!(env.contains("image: '{{ index .Values.images \"web\" }}'"));
    assert!(!env.contains("image: redis:7-alpine"), "the machine's own image is the value");
    assert!(env.contains("image: busybox:"), "Isoloom's probe stays as it is");
    // Every manifest of the plain output is in the chart, the namespace aside.
    let plain = file(&files, "kubernetes/environment.yaml")?;
    assert_eq!(plain.matches("\nkind: ").count() - 1, env.matches("\nkind: ").count());
    Ok(())
}

#[test]
fn the_checks_are_helm_tests_and_the_scripts_come_from_the_chart() -> Outcome {
    let (dir, files) = kubernetes("hello-stack")?;
    let tests = file(&files, "chart/templates/tests/checks.yaml")?;
    assert_eq!(tests.matches("helm.sh/hook: test").count(), 3);
    assert_eq!(tests.matches("kind: Pod").count(), 3, "pods, so `helm test --logs` finds them");
    assert!(tests.contains("name: isoloom-check\n") && tests.contains("restartPolicy: Never"));
    assert!(tests.contains("networks.sh: '{{ .Files.Get \"files/checks/networks.sh\" | b64enc }}'"));
    assert_eq!(file(&files, "chart/files/checks/web.sh")?, file(&files, "kubernetes/checks/web.sh")?);
    assert!(!tests.contains("exec-cache"), "exec checks stay with isoloom test");

    let scripts = file(&files, "chart/templates/scripts.yaml")?;
    assert!(scripts.contains("build__cache__seed.sh: '{{ required"), "{scripts}");
    let copies = project_copies(&files, &dir).unwrap();
    let seed = copies.iter().find(|f| f.path.ends_with("chart/files/project/build__cache__seed.sh")).unwrap();
    assert_eq!(seed.contents, std::fs::read_to_string(dir.join("build/cache/seed.sh")).unwrap());
    assert_eq!(copies.len(), 2, "the init job and the check script");

    let notes = file(&files, "chart/templates/NOTES.txt")?;
    assert!(notes.contains("helm test {{ .Release.Name }} -n {{ .Release.Namespace }}"), "{notes}");
    Ok(())
}

#[test]
fn a_missing_script_is_left_to_validate() -> Outcome {
    let (dir, files) = kubernetes("pivot-dmz")?;
    assert!(project_copies(&files, &dir).unwrap().is_empty(), "its scripts aren't part of the example");
    Ok(())
}

#[test]
fn inputs_and_constraints_reach_the_values() -> Outcome {
    let spec = parse(
        "version: 1
name: shop
inputs: { API_KEY: { sensitive: false } }
networks: { app: { cidr: 10.60.0.0/24 } }
machines:
  web:
    networks: { app: 10 }
    ports: { web: 80 }
    environment: { API_KEY: { input: API_KEY } }
    app: { image: nginx }
message: |
  Open {{ machines.web.addresses.app }}.
constraints:
  registries: { docker.io: registry.corp/hub }
",
    )
    .unwrap();
    let files = generate(&Lab::new(spec), Target::Kubernetes).unwrap();
    let values: serde_yaml_ng::Value = serde_yaml_ng::from_str(file(&files, "chart/values.yaml")?).unwrap();
    assert_eq!(values["images"]["web"].as_str(), Some("registry.corp/hub/library/nginx"));
    assert_eq!(values["inputs"]["API_KEY"].as_str(), Some(""));
    let secret = file(&files, "chart/templates/inputs.yaml")?;
    assert!(secret.contains("name: isoloom-inputs") && secret.contains("API_KEY: '{{ get .Values.inputs \"API_KEY\" | toString | b64enc }}'"));
    let notes = file(&files, "chart/templates/NOTES.txt")?;
    assert!(
        notes.contains("isoloom message --target kubernetes") && !notes.contains("machines.web"),
        "{notes}"
    );
    Ok(())
}
