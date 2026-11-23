//! The Helm chart beside the Kubernetes manifests: the same environment, images and inputs as
//! values, checks as `helm test`, the project's scripts copied in.

use std::path::PathBuf;

use isoloom_spec::{Target, parse};
use isoloom_targets::{GeneratedFile, Lab, generate, project_copies};

use isoloom_test_support::{manifest, manifest_names, yaml, yaml_documents, yaml_keys_at, yaml_str, yaml_values};

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
    let chart = yaml(file(&files, "chart/Chart.yaml")?);
    assert_eq!(["apiVersion", "name", "version"].map(|k| yaml_str(&chart, k)), ["v2", "hello-stack", "0.1.0"]);
    let values: serde_yaml_ng::Value = serde_yaml_ng::from_str(file(&files, "chart/values.yaml")?)?;
    assert_eq!(values["images"]["cache"].as_str(), Some("redis:7-alpine"));
    assert_eq!(values["images"]["web"].as_str(), Some("localhost/isoloom/hello-stack-web"));

    // The template's values are quoted: it reads as YAML before Helm renders it.
    let env = yaml_documents(file(&files, "chart/templates/environment.yaml")?);
    assert_eq!(manifest_names(&env, "Namespace"), Vec::<String>::new(), "the release's namespace");
    let images: Vec<String> = env.iter().flat_map(|d| yaml_values(d, "image")).collect();
    for machine in ["cache", "web"] {
        let value = format!("{{{{ index .Values.images \"{machine}\" }}}}");
        assert!(images.contains(&value), "{machine}: {images:?}");
    }
    assert!(!images.iter().any(|i| i == "redis:7-alpine"), "the machine's own image is the value");
    assert!(images.iter().any(|i| i.starts_with("busybox:")), "Isoloom's probe stays as it is");
    // Every manifest of the plain output is in the chart, the namespace aside.
    let plain = yaml_documents(file(&files, "kubernetes/environment.yaml")?);
    assert_eq!(plain.len() - 1, env.len());
    Ok(())
}

#[test]
fn the_checks_are_helm_tests_and_the_scripts_come_from_the_chart() -> Outcome {
    let (dir, lab, files) = kubernetes("hello-stack")?;
    let tests = yaml_documents(file(&files, "chart/templates/tests/checks.yaml")?);
    // Pods, so `helm test --logs` finds them, each a test hook.
    let pods = manifest_names(&tests, "Pod");
    assert_eq!(pods.len(), 3, "{pods:?}");
    for pod in &pods {
        assert_eq!(yaml_str(manifest(&tests, "Pod", pod), "metadata.annotations.`helm.sh/hook`"), "test", "{pod}");
    }
    assert_eq!(yaml_str(manifest(&tests, "Pod", "isoloom-check"), "spec.restartPolicy"), "Never");
    let runners = manifest(&tests, "ConfigMap", "isoloom-check-runners");
    assert_eq!(
        yaml_str(runners, "binaryData.`isoloom-networks.sh`"),
        "{{ .Files.Get \"files/checks/isoloom-networks.sh\" | b64enc }}"
    );
    assert_eq!(file(&files, "chart/files/checks/web.sh")?, file(&files, "kubernetes/checks/web.sh")?);
    let carried = yaml_keys_at(runners, "binaryData");
    assert!(
        !carried.iter().any(|k| k.starts_with("isoloom-exec-")),
        "exec checks stay with the runner: {carried:?}"
    );

    let scripts = yaml_documents(file(&files, "chart/templates/scripts.yaml")?);
    let seed_value = yaml_str(manifest(&scripts, "ConfigMap", "isoloom-scripts"), "binaryData.`build__cache__seed.sh`");
    assert!(seed_value.starts_with("{{ required"), "{seed_value}");
    let copies = project_copies(&lab, &files, &dir)?;
    // At its path in the project, where the kustomization reads it too.
    let seed = copies
        .iter()
        .find(|f| f.path.ends_with("chart/files/project/build/cache/seed.sh"))
        .ok_or("no copy of seed.sh")?;
    assert_eq!(seed.contents, std::fs::read_to_string(dir.join("build/cache/seed.sh"))?);
    assert_eq!(copies.len(), 2, "the init job and the check script");

    let notes = file(&files, "chart/templates/NOTES.txt")?;
    assert!(notes.contains("helm test {{ .Release.Name }} -n {{ .Release.Namespace }}"), "{notes}");
    Ok(())
}

#[test]
fn a_missing_script_fails_saying_which_and_why_once() -> Outcome {
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
    // Each cause says its own part, so the chain names the script and why once each.
    assert_eq!(
        chain,
        [
            "a project file the output copies can't be read",
            "checks/dmz-reachable.sh (copied into the Helm chart)",
            "it doesn't exist in the project folder",
        ],
    );
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
    let files = generate(&Lab::new(spec), Target::Kubernetes)?;
    let values: serde_yaml_ng::Value = serde_yaml_ng::from_str(file(&files, "chart/values.yaml")?)?;
    assert_eq!(values["images"]["web"].as_str(), Some("registry.corp/hub/library/nginx"));
    assert_eq!(values["inputs"]["API_KEY"].as_str(), Some(""));
    let secret = yaml_documents(file(&files, "chart/templates/inputs.yaml")?);
    assert_eq!(
        yaml_str(manifest(&secret, "Secret", "isoloom-inputs"), "data.API_KEY"),
        "{{ get .Values.inputs \"API_KEY\" | toString | b64enc }}"
    );
    let notes = file(&files, "chart/templates/NOTES.txt")?;
    assert!(
        notes.contains("isoloom message --target kubernetes") && !notes.contains("machines.web"),
        "{notes}"
    );
    Ok(())
}
