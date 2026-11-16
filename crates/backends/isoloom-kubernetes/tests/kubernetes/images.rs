//! The images the pods run, through the mirrors but the built ones, and the chart's templates
//! of them, braces kept.

use std::path::Path;

use isoloom_kubernetes::KubernetesBackend;
use isoloom_render::backend::Backend;
use isoloom_render::container::{CHECK_IMAGE, PROBE_IMAGE, UTILITY_IMAGE};
use isoloom_test_support::assert_every;

use crate::testing::{MIRRORED, Outcome, file, parsed};

/// Every container's `image` in the manifests `yaml` (each document, init containers too), in
/// order.
fn images(yaml: &str) -> Vec<String> {
    fn walk(v: &serde_yaml_ng::Value, out: &mut Vec<String>) {
        match v {
            serde_yaml_ng::Value::Mapping(m) => {
                for (key, value) in m {
                    match (key.as_str(), value.as_str()) {
                        (Some("image"), Some(image)) => out.push(image.to_owned()),
                        _ => walk(value, out),
                    }
                }
            }
            serde_yaml_ng::Value::Sequence(items) => items.iter().for_each(|i| walk(i, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for doc in isoloom_test_support::yaml_documents(yaml) {
        walk(&doc, &mut out);
    }
    out
}

/// Every value of the environment variable `name` of a container in the manifests `yaml`.
fn env_values(yaml: &str, name: &str) -> Vec<String> {
    fn walk(v: &serde_yaml_ng::Value, name: &str, out: &mut Vec<String>) {
        match v {
            serde_yaml_ng::Value::Mapping(m) => {
                if m.get("name").and_then(serde_yaml_ng::Value::as_str) == Some(name)
                    && let Some(value) = m.get("value").and_then(serde_yaml_ng::Value::as_str)
                {
                    out.push(value.to_owned());
                }
                m.values().for_each(|value| walk(value, name, out));
            }
            serde_yaml_ng::Value::Sequence(items) => items.iter().for_each(|i| walk(i, name, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for doc in isoloom_test_support::yaml_documents(yaml) {
        walk(&doc, name, &mut out);
    }
    out
}

#[test]
fn every_pulled_image_goes_through_the_mirrors_but_the_built_ones() -> Outcome {
    let files = KubernetesBackend.render(&parsed(MIRRORED)?)?;
    let env = file(&files, "kubernetes/environment.yaml")?;
    let pulled = images(env);
    // Every image a container runs: the mirror's, but the built one, which is local.
    assert_every!(&pulled, |i: &&String| i.starts_with("registry.corp/hub/") || *i == "localhost/isoloom/shop-db");
    for wanted in [
        "registry.corp/hub/library/nginx".to_owned(),
        format!("registry.corp/hub/library/{PROBE_IMAGE}"),
        format!("registry.corp/hub/library/{UTILITY_IMAGE}"),
        "localhost/isoloom/shop-db".to_owned(),
    ] {
        assert!(pulled.contains(&wanted), "{wanted}: {pulled:?}");
    }
    // How to build the local image, said in the file's comment.
    assert!(env.lines().any(|l| l == "#   docker build -t localhost/isoloom/shop-db db"), "{env}");
    let jobs = file(&files, "checks/job.yaml")?;
    // A Job per position, each on the mirror's check image.
    let checks = images(jobs);
    assert_every!(&checks, |i: &&String| **i == format!("registry.corp/hub/{CHECK_IMAGE}"));
    Ok(())
}

#[test]
fn the_chart_templates_the_images_and_keeps_braces_as_they_are() -> Outcome {
    let files = KubernetesBackend.render(&parsed(MIRRORED)?)?;
    let env = file(&files, "chart/templates/environment.yaml")?;
    // The template is YAML before Helm renders it: its images and values read as such.
    let images = images(env);
    for wanted in [
        "{{ index .Values.images \"web\" }}".to_owned(),
        "{{ index .Values.images \"db\" }}".to_owned(),
        format!("registry.corp/hub/library/{UTILITY_IMAGE}"),
    ] {
        assert!(images.contains(&wanted), "{wanted}: {images:?}");
    }
    assert_eq!(env_values(env, "GREETING"), ["hi {{ `{{` }} there }}"]);
    let values: serde_yaml_ng::Value = serde_yaml_ng::from_str(file(&files, "chart/values.yaml")?)?;
    assert_eq!(
        values.get("images").and_then(|i| i.get("web")).and_then(serde_yaml_ng::Value::as_str),
        Some("registry.corp/hub/library/nginx")
    );
    assert_eq!(
        values.get("images").and_then(|i| i.get("db")).and_then(serde_yaml_ng::Value::as_str),
        Some("localhost/isoloom/shop-db")
    );
    Ok(())
}

#[test]
fn every_chart_template_is_yaml_before_helm_renders_it() -> Outcome {
    let files = KubernetesBackend.render(&parsed(MIRRORED)?)?;
    let templates = files
        .iter()
        .filter(|f| f.path.contains("/chart/templates/") && Path::new(&f.path).extension().is_some_and(|e| e == "yaml"));
    for f in templates {
        for doc in f.contents.split("---\n") {
            serde_yaml_ng::from_str::<serde_yaml_ng::Value>(doc).map_err(|e| format!("{}: {e}", f.path))?;
        }
    }
    Ok(())
}

#[test]
fn braces_survive_a_double_quoted_string() -> Outcome {
    let spec = MIRRORED.replace("'hi {{ there }}'", "\"x\\t{{ y }}\"");
    let files = KubernetesBackend.render(&parsed(&spec)?)?;
    let env = file(&files, "chart/templates/environment.yaml")?;
    assert_eq!(env_values(env, "GREETING"), ["x\t{{ `{{` }} y }}"], "{env}");
    Ok(())
}

#[test]
fn a_build_with_its_dockerfile_and_arguments_and_a_plain_message() -> Outcome {
    let spec = MIRRORED
        .replace(
            "app: { build: db }",
            "app: { build: { context: db, dockerfile: db/Dockerfile.dev, args: { V: \"1 2\" } } }",
        )
        .replace("constraints:", "message: Open the shop.\nconstraints:");
    let files = KubernetesBackend.render(&parsed(&spec)?)?;
    let env = file(&files, "kubernetes/environment.yaml")?;
    assert!(
        env.lines()
            .any(|l| l == "#   docker build -t localhost/isoloom/shop-db -f db/Dockerfile.dev --build-arg 'V=1 2' db"),
        "{env}"
    );
    assert_eq!(
        file(&files, "chart/templates/NOTES.txt")?,
        "Open the shop.\n\nChecks: helm test {{ .Release.Name }} -n {{ .Release.Namespace }}\n"
    );
    Ok(())
}
