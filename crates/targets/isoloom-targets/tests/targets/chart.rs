//! The Helm chart beside the Kubernetes manifests: the same environment, images and inputs as
//! values, checks as `helm test`, the project's scripts copied in.

use std::path::PathBuf;

use isoloom_spec::{Target, parse};
use isoloom_targets::{GeneratedFile, Lab, generate, project_copies};

use crate::testing::{Outcome, example, examples, file};

/// The example `name`'s folder, its environment and its Kubernetes files.
fn kubernetes(name: &str) -> Outcome<(PathBuf, Lab, Vec<GeneratedFile>)> {
    let lab = example(name)?;
    let files = generate(&lab, Target::Kubernetes)?;
    Ok((examples().join(name), lab, files))
}

#[test]
fn the_chart_is_the_same_environment_with_images_as_values() -> Outcome {
    let (_, _, files) = kubernetes("hello-stack")?;
    let chart = file(&files, "chart/Chart.yaml")?;
    assert!(
        chart.contains("apiVersion: v2\nname: hello-stack\n") && chart.contains("version: 0.1.0"),
        "{chart}"
    );
    let values: serde_yaml_ng::Value = serde_yaml_ng::from_str(file(&files, "chart/values.yaml")?).unwrap();
    assert_eq!(values["images"]["cache"].as_str(), Some("redis:7-alpine"));
    assert_eq!(values["images"]["web"].as_str(), Some("localhost/isoloom/hello-stack-web"));

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
    let (dir, lab, files) = kubernetes("hello-stack")?;
    let tests = file(&files, "chart/templates/tests/checks.yaml")?;
    assert_eq!(tests.matches("helm.sh/hook: test").count(), 3);
    assert_eq!(tests.matches("kind: Pod").count(), 3, "pods, so `helm test --logs` finds them");
    assert!(tests.contains("name: isoloom-check\n") && tests.contains("restartPolicy: Never"));
    assert!(tests.contains("isoloom-networks.sh: '{{ .Files.Get \"files/checks/isoloom-networks.sh\" | b64enc }}'"));
    assert_eq!(file(&files, "chart/files/checks/web.sh")?, file(&files, "kubernetes/checks/web.sh")?);
    assert!(!tests.contains("isoloom-exec-cache"), "exec checks stay with the runner");

    let scripts = file(&files, "chart/templates/scripts.yaml")?;
    assert!(scripts.contains("build__cache__seed.sh: '{{ required"), "{scripts}");
    let copies = project_copies(&lab, &files, &dir).unwrap();
    // At its path in the project, where the kustomization reads it too.
    let seed = copies.iter().find(|f| f.path.ends_with("chart/files/project/build/cache/seed.sh")).unwrap();
    assert_eq!(seed.contents, std::fs::read_to_string(dir.join("build/cache/seed.sh")).unwrap());
    assert_eq!(copies.len(), 2, "the init job and the check script");

    let notes = file(&files, "chart/templates/NOTES.txt")?;
    assert!(notes.contains("helm test {{ .Release.Name }} -n {{ .Release.Namespace }}"), "{notes}");
    Ok(())
}

#[test]
fn a_missing_script_fails_with_the_read_error() -> Outcome {
    let (dir, lab, files) = kubernetes("dmz-jump")?;
    assert_eq!(project_copies(&lab, &files, &dir).unwrap().len(), 2, "its two check scripts");
    let elsewhere = dir.join("a folder without the scripts");
    let e = project_copies(&lab, &files, &elsewhere).unwrap_err();
    let mut chain = Vec::new();
    let mut next: Option<&dyn std::error::Error> = Some(&e);
    while let Some(cause) = next {
        chain.push(cause.to_string());
        next = cause.source();
    }
    let chain = chain.join(": ");
    assert!(chain.contains("checks/") && chain.contains("No such file"), "{chain}");
    Ok(())
}

#[test]
fn only_the_kubernetes_files_copy_the_scripts() -> Outcome {
    let (dir, lab, _) = kubernetes("hello-stack")?;
    let compose = generate(&lab, Target::Docker)?;
    assert!(project_copies(&lab, &compose, &dir).unwrap().is_empty(), "no Kubernetes files, nothing to copy");
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
