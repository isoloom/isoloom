//! The Kubernetes backend's rendering of a few examples, as snapshots (the manifests with their
//! kustomizations and checks, and the Helm chart), its refusals and its determinism.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::fmt::Write;
use std::path::Path;

use isoloom_plan::{Environment, Feature};
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

fn plan(example: &str) -> Environment {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(example);
    let spec = isoloom_spec::load(&dir).expect("the example loads");
    assert!(isoloom_spec::validate(&spec).is_empty(), "the example is valid");
    Environment::build(&spec, Target::Kubernetes, None)
}

/// The files the backend renders for an example, from its plan.
fn render(example: &str) -> Vec<GeneratedFile> {
    isoloom_kubernetes::render(&plan(example)).expect("Kubernetes renders the example")
}

/// The files whose path passes `keep`, one after the other under their path.
fn joined(files: &[GeneratedFile], keep: impl Fn(&str) -> bool) -> String {
    let mut out = String::new();
    for f in files.iter().filter(|f| keep(&f.path)) {
        let _ = write!(out, "=== {}\n{}\n", f.path, f.contents);
    }
    out
}

fn manifests(example: &str) -> String {
    joined(&render(example), |p| !p.contains("/chart/"))
}

fn chart(example: &str) -> String {
    joined(&render(example), |p| p.contains("/chart/"))
}

#[test]
fn hello_stack_manifests() {
    insta::assert_snapshot!(manifests("hello-stack"));
}

#[test]
fn hello_stack_chart() {
    insta::assert_snapshot!(chart("hello-stack"));
}

#[test]
fn segmented_manifests() {
    insta::assert_snapshot!(manifests("segmented"));
}

#[test]
fn segmented_chart() {
    insta::assert_snapshot!(chart("segmented"));
}

#[test]
fn shared_files_manifests() {
    insta::assert_snapshot!(manifests("shared-files"));
}

#[test]
fn shared_files_chart() {
    insta::assert_snapshot!(chart("shared-files"));
}

#[test]
fn tuned_web_is_refused_for_its_ulimits() {
    let err = isoloom_kubernetes::render(&plan("tuned-web")).expect_err("refused");
    insta::assert_snapshot!(err.to_string());
}

#[test]
fn the_refusal_comes_from_the_plan_without_rendering() {
    let backend = isoloom_kubernetes::KubernetesBackend;
    let why = backend.refusal(&plan("tuned-web")).expect("refused");
    assert_eq!(why, "machine `web` sets `ulimits`: Kubernetes has no per-container process limits");
    assert!(backend.refusal(&plan("hello-stack")).is_none());
}

#[test]
fn rendering_twice_gives_the_same_files() {
    for example in ["hello-stack", "segmented", "shared-files", "supplier-portal-api", "udp-dns"] {
        assert_eq!(render(example), render(example), "{example}");
    }
}

fn parsed(yaml: &str) -> Environment {
    let spec = isoloom_spec::parse(yaml).expect("parses");
    assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
    Environment::build(&spec, Target::Kubernetes, None)
}

fn file<'a>(files: &'a [GeneratedFile], suffix: &str) -> &'a str {
    &files
        .iter()
        .find(|f| f.path.ends_with(suffix))
        .unwrap_or_else(|| panic!("no {suffix}"))
        .contents
}

const MIRRORED: &str = "version: 1
name: shop
networks: { app: { cidr: 10.60.0.0/24 } }
machines:
  web:
    networks: { app: 10 }
    services: [{ port: 80 }]
    environment: { GREETING: 'hi {{ there }}' }
    docker: { image: nginx }
  db:
    networks: { app: 20 }
    services: [{ port: 5432 }]
    depends_on: [web]
    docker: { build: db }
constraints:
  registries: { docker.io: registry.corp/hub }
";

#[test]
fn every_pulled_image_goes_through_the_mirrors_but_the_built_ones() {
    let files = isoloom_kubernetes::render(&parsed(MIRRORED)).unwrap();
    let env = file(&files, "kubernetes/environment.yaml");
    assert!(env.contains("image: registry.corp/hub/library/nginx\n"), "{env}");
    assert!(env.contains("image: registry.corp/hub/library/busybox:1.37.0-musl\n"), "Isoloom's probe too");
    assert!(env.contains("image: registry.corp/hub/library/alpine:3.20\n"), "and its waits");
    assert!(env.contains("image: isoloom/shop-db\n"), "a built image is local");
    assert!(env.contains("#   docker build -t isoloom/shop-db db\n"), "{env}");
    let jobs = file(&files, "checks/job.yaml");
    assert!(jobs.contains("image: registry.corp/hub/curlimages/curl:8.11.1\n"), "{jobs}");
}

#[test]
fn the_chart_templates_the_images_and_keeps_braces_as_they_are() {
    let files = isoloom_kubernetes::render(&parsed(MIRRORED)).unwrap();
    let env = file(&files, "chart/templates/environment.yaml");
    assert!(env.contains("image: '{{ index .Values.images \"web\" }}'\n"), "{env}");
    assert!(env.contains("image: '{{ index .Values.images \"db\" }}'\n"), "{env}");
    assert!(env.contains("image: registry.corp/hub/library/alpine:3.20\n"), "the waits are Isoloom's");
    assert!(env.contains("value: hi {{ `{{` }} there }}"), "{env}");
    let values: serde_yaml_ng::Value = serde_yaml_ng::from_str(file(&files, "chart/values.yaml")).unwrap();
    assert_eq!(values["images"]["web"].as_str(), Some("registry.corp/hub/library/nginx"));
    assert_eq!(values["images"]["db"].as_str(), Some("isoloom/shop-db"));
    // Every template is YAML before Helm renders it.
    for f in files
        .iter()
        .filter(|f| f.path.contains("/chart/templates/") && std::path::Path::new(&f.path).extension().is_some_and(|e| e == "yaml"))
    {
        for doc in f.contents.split("---\n") {
            serde_yaml_ng::from_str::<serde_yaml_ng::Value>(doc).unwrap_or_else(|e| panic!("{}: {e}", f.path));
        }
    }
}

#[test]
fn braces_survive_a_double_quoted_string() {
    let spec = MIRRORED.replace("'hi {{ there }}'", "\"x\\t{{ y }}\"");
    let files = isoloom_kubernetes::render(&parsed(&spec)).unwrap();
    let env = file(&files, "chart/templates/environment.yaml");
    assert!(env.contains("value: \"x\\t{{ `{{` }} y }}\""), "{env}");
}

#[test]
fn each_unsupported_feature_is_refused_with_its_reason() {
    let base = "version: 1\nname: r\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  web:\n    networks: { lan: 10 }\n    docker: { image: nginx }\n";
    let cases = [
        (
            "    aliases: [www.lab.test]\n",
            "machine `aliases` (dotted DNS names) aren't Kubernetes Service names",
        ),
        (
            "    ulimits: { nofile: 1024 }\n",
            "machine `web` sets `ulimits`: Kubernetes has no per-container process limits",
        ),
    ];
    for (extra, why) in cases {
        let err = isoloom_kubernetes::render(&parsed(&format!("{base}{extra}"))).expect_err("refused");
        assert_eq!(err.to_string(), format!("kubernetes: {why}"));
    }
    let tc = base.replace("{ cidr: 10.9.0.0/24 }", "{ cidr: 10.9.0.0/24, tc: { delay: 50ms } }");
    let err = isoloom_kubernetes::render(&parsed(&tc)).expect_err("refused");
    assert_eq!(
        err.to_string(),
        "kubernetes: link impairment (`tc`) needs a router in the path; Kubernetes has none"
    );
    // A gateway machine: the plan words it for hybrid, Kubernetes in its own terms.
    let gateway = base
        .replace("{ cidr: 10.9.0.0/24 }", "{ cidr: 10.9.0.0/24, gateway: web }")
        .replace("{ lan: 10 }", "{ lan: 1 }");
    let env = parsed(&gateway);
    let why = "networks with a `gateway` machine on Kubernetes come later";
    let err = isoloom_kubernetes::render(&env).expect_err("refused");
    assert_eq!(err.to_string(), format!("kubernetes: {why}"));
    assert_eq!(isoloom_kubernetes::KubernetesBackend.refusal(&env).as_deref(), Some(why));
}

#[test]
fn a_build_with_its_dockerfile_and_arguments_and_a_plain_message() {
    let spec = MIRRORED
        .replace(
            "docker: { build: db }",
            "docker: { build: db, dockerfile: db/Dockerfile.dev, args: { V: \"1 2\" } }",
        )
        .replace("constraints:", "message: Open the shop.\nconstraints:");
    let files = isoloom_kubernetes::render(&parsed(&spec)).unwrap();
    let env = file(&files, "kubernetes/environment.yaml");
    assert!(
        env.contains("#   docker build -t isoloom/shop-db -f db/Dockerfile.dev --build-arg 'V=1 2' db\n"),
        "{env}"
    );
    assert_eq!(
        file(&files, "chart/templates/NOTES.txt"),
        "Open the shop.\n\nChecks: helm test {{ .Release.Name }} -n {{ .Release.Namespace }}\n"
    );
    assert_eq!(
        isoloom_kubernetes::KubernetesBackend.capabilities(),
        isoloom_plan::Capabilities::of([
            Feature::Ipv6,
            Feature::Ipv6Only,
            Feature::Ipv6Auto,
            Feature::Secrets,
            Feature::OwnedProjectFiles
        ])
    );
}
