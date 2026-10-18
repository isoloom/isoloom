//! The Kubernetes backend's rendering of a few examples, as snapshots (the manifests with their
//! kustomizations and checks, and the Helm chart), its refusals and its determinism.

use std::error::Error;
use std::path::Path;

use isoloom_kubernetes::KubernetesBackend;
use isoloom_plan::{Environment, Lab};
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The environment of `spec` (the authored format), which must be valid, planned for Kubernetes.
fn planned(spec: isoloom_spec::Spec) -> Outcome<Environment> {
    let lab = Lab::new(spec);
    let problems = lab.problems();
    if !problems.is_empty() {
        return Err(format!("the spec isn't valid: {problems:?}").into());
    }
    Ok(Environment::build(&lab.for_target(Target::Kubernetes), Target::Kubernetes, None))
}

/// An example of the project, as written.
fn load(example: &str) -> Outcome<isoloom_spec::Spec> {
    Ok(isoloom_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(example),
    )?)
}

/// An example of the project, planned for Kubernetes.
fn plan(example: &str) -> Outcome<Environment> {
    planned(load(example)?)
}

/// `tuned-web` without its TPM, which makes it a VM only: its container form, which Kubernetes
/// still refuses.
fn tuned_web_container() -> Outcome<Environment> {
    let mut tuned = load("tuned-web")?;
    let web = tuned.machines.get_mut("web").ok_or("no web machine")?;
    web.hardware.tpm = isoloom_spec::syntax::Tpm::Auto;
    planned(tuned)
}

/// A spec written in a test, planned for Kubernetes.
fn parsed(yaml: &str) -> Outcome<Environment> {
    planned(isoloom_spec::parse(yaml)?)
}

/// The files the backend renders for an example, from its plan.
fn render(example: &str) -> Outcome<Vec<GeneratedFile>> {
    Ok(KubernetesBackend.render(&plan(example)?)?)
}

/// The files whose path passes `keep`, one after the other under their path.
fn joined(files: &[GeneratedFile], keep: impl Fn(&str) -> bool) -> String {
    let parts: Vec<String> = files
        .iter()
        .filter(|f| keep(&f.path))
        .map(|f| format!("=== {}\n{}\n", f.path, f.contents))
        .collect();
    parts.concat()
}

fn manifests(example: &str) -> Outcome<String> {
    Ok(joined(&render(example)?, |p| !p.contains("/chart/")))
}

fn chart(example: &str) -> Outcome<String> {
    Ok(joined(&render(example)?, |p| p.contains("/chart/")))
}

/// The text of the one file among `files` whose path ends with `suffix`.
fn file<'a>(files: &'a [GeneratedFile], suffix: &str) -> Outcome<&'a str> {
    let found = files.iter().find(|f| f.path.ends_with(suffix)).ok_or_else(|| format!("no {suffix}"))?;
    Ok(&found.contents)
}

#[test]
fn a_machine_runs_its_own_process() -> Outcome {
    let fixture = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/own-process.yml"))?;
    let files = KubernetesBackend.render(&parsed(&fixture)?)?;
    insta::assert_snapshot!(joined(&files, |p| p.ends_with("environment.yaml")));
    Ok(())
}

#[test]
fn hello_stack_manifests() -> Outcome {
    insta::assert_snapshot!(manifests("hello-stack")?);
    Ok(())
}

#[test]
fn hello_stack_chart() -> Outcome {
    insta::assert_snapshot!(chart("hello-stack")?);
    Ok(())
}

#[test]
fn segmented_manifests() -> Outcome {
    insta::assert_snapshot!(manifests("segmented")?);
    Ok(())
}

#[test]
fn segmented_chart() -> Outcome {
    insta::assert_snapshot!(chart("segmented")?);
    Ok(())
}

#[test]
fn shared_files_manifests() -> Outcome {
    insta::assert_snapshot!(manifests("shared-files")?);
    Ok(())
}

#[test]
fn shared_files_chart() -> Outcome {
    insta::assert_snapshot!(chart("shared-files")?);
    Ok(())
}

#[test]
fn tuned_web_is_refused_for_its_ulimits() -> Outcome {
    let err = KubernetesBackend.render(&tuned_web_container()?).err().ok_or("rendered")?;
    insta::assert_snapshot!(err.to_string());
    Ok(())
}

#[test]
fn the_refusal_comes_from_the_plan_without_rendering() -> Outcome {
    let why = KubernetesBackend.refusal(&tuned_web_container()?).ok_or("not refused")?;
    assert_eq!(why, "machine `web` sets `ulimits`: Kubernetes has no per-container process limits");
    assert!(KubernetesBackend.refusal(&plan("hello-stack")?).is_none());
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for example in ["hello-stack", "segmented", "shared-files", "supplier-portal-api", "udp-dns"] {
        assert_eq!(render(example)?, render(example)?, "{example}");
    }
    Ok(())
}

const MIRRORED: &str = "version: 1
name: shop
networks: { app: { cidr: 10.60.0.0/24 } }
machines:
  web:
    networks: { app: 10 }
    ports: { web: 80 }
    environment: { GREETING: 'hi {{ there }}' }
    app: { image: nginx }
  db:
    networks: { app: 20 }
    ports: { db: 5432 }
    depends_on: [web]
    app: { build: db }
constraints:
  registries: { docker.io: registry.corp/hub }
";

#[test]
fn every_pulled_image_goes_through_the_mirrors_but_the_built_ones() -> Outcome {
    let files = KubernetesBackend.render(&parsed(MIRRORED)?)?;
    let env = file(&files, "kubernetes/environment.yaml")?;
    assert!(env.contains("image: registry.corp/hub/library/nginx\n"), "{env}");
    assert!(env.contains("image: registry.corp/hub/library/busybox:1.37.0-musl\n"), "Isoloom's probe too");
    assert!(env.contains("image: registry.corp/hub/library/alpine:3.20\n"), "and its waits");
    assert!(env.contains("image: isoloom/shop-db\n"), "a built image is local");
    assert!(env.contains("#   docker build -t isoloom/shop-db db\n"), "{env}");
    let jobs = file(&files, "checks/job.yaml")?;
    assert!(jobs.contains("image: registry.corp/hub/curlimages/curl:8.11.1\n"), "{jobs}");
    Ok(())
}

#[test]
fn the_chart_templates_the_images_and_keeps_braces_as_they_are() -> Outcome {
    let files = KubernetesBackend.render(&parsed(MIRRORED)?)?;
    let env = file(&files, "chart/templates/environment.yaml")?;
    assert!(env.contains("image: '{{ index .Values.images \"web\" }}'\n"), "{env}");
    assert!(env.contains("image: '{{ index .Values.images \"db\" }}'\n"), "{env}");
    assert!(env.contains("image: registry.corp/hub/library/alpine:3.20\n"), "the waits are Isoloom's");
    assert!(env.contains("value: hi {{ `{{` }} there }}"), "{env}");
    let values: serde_yaml_ng::Value = serde_yaml_ng::from_str(file(&files, "chart/values.yaml")?)?;
    assert_eq!(
        values.get("images").and_then(|i| i.get("web")).and_then(serde_yaml_ng::Value::as_str),
        Some("registry.corp/hub/library/nginx")
    );
    assert_eq!(
        values.get("images").and_then(|i| i.get("db")).and_then(serde_yaml_ng::Value::as_str),
        Some("isoloom/shop-db")
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
    assert!(env.contains("value: \"x\\t{{ `{{` }} y }}\""), "{env}");
    Ok(())
}

/// A spec with one machine, `web`, on one network, `lan`, and `extra` lines for the machine.
const BASE: &str = "version: 1\nname: r\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  web:\n    networks: { lan: 10 }\n    app: { image: nginx }\n";

/// What rendering `yaml` reports, as every backend reports it.
fn refusal(yaml: &str) -> Outcome<String> {
    let err = KubernetesBackend.render(&parsed(yaml)?).err().ok_or("rendered")?;
    Ok(err.to_string())
}

#[test]
fn each_unsupported_feature_is_refused_with_its_reason() -> Outcome {
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
        assert_eq!(refusal(&format!("{BASE}{extra}"))?, format!("kubernetes: {why}"));
    }
    let tc = BASE.replace("{ cidr: 10.9.0.0/24 }", "{ cidr: 10.9.0.0/24, tc: { delay: 50ms } }");
    assert_eq!(
        refusal(&tc)?,
        "kubernetes: link impairment (`tc`) needs a router in the path; Kubernetes has none"
    );
    Ok(())
}

#[test]
fn a_gateway_machine_is_refused_in_kubernetes_words() -> Outcome {
    // The plan words it for hybrid; Kubernetes in its own terms.
    let gateway = BASE
        .replace("{ cidr: 10.9.0.0/24 }", "{ cidr: 10.9.0.0/24, gateway: web }")
        .replace("{ lan: 10 }", "{ lan: 1 }");
    let why = "networks with a `gateway` machine on Kubernetes come later";
    assert_eq!(refusal(&gateway)?, format!("kubernetes: {why}"));
    assert_eq!(KubernetesBackend.refusal(&parsed(&gateway)?).as_deref(), Some(why));
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
        env.contains("#   docker build -t isoloom/shop-db -f db/Dockerfile.dev --build-arg 'V=1 2' db\n"),
        "{env}"
    );
    assert_eq!(
        file(&files, "chart/templates/NOTES.txt")?,
        "Open the shop.\n\nChecks: helm test {{ .Release.Name }} -n {{ .Release.Namespace }}\n"
    );
    Ok(())
}

/// A secret with an owner is copied out of the Secret volume by an init container, into a
/// memory-backed `emptyDir`, and mounted from there; one without stays on the Secret's volume.
#[test]
fn a_secret_with_an_owner_is_copied_into_memory_with_it() -> Outcome {
    let env = parsed(
        "version: 1\nname: s\nsecrets: { db-password: { env: DB_PASSWORD }, api-key: { env: API_KEY } }\nnetworks: { lan: { cidr: 10.20.0.0/24 } }\nmachines:\n  db:\n    networks: { lan: 10 }\n    app: { image: postgres }\n    files:\n      - { path: /run/secrets/pw, secret: db-password, uid: 999, gid: 998, mode: '0400' }\n      - { path: /run/secrets/api-key, secret: api-key }\n",
    )?;
    let files = KubernetesBackend.render(&env)?;
    let manifests = file(&files, "kubernetes/environment.yaml")?;
    assert!(
        manifests.contains("cp /isoloom-secrets/in/db-password /isoloom-secrets/out/db-password && chown 999:998 /isoloom-secrets/out/db-password && chmod 0400 /isoloom-secrets/out/db-password"),
        "{manifests}"
    );
    assert!(
        manifests.contains("- name: isoloom-secrets-owned\n        emptyDir:\n          medium: Memory\n"),
        "{manifests}"
    );
    assert!(
        manifests.contains("- name: isoloom-secrets-owned\n          mountPath: /run/secrets/pw\n          readOnly: true\n          subPath: db-password\n"),
        "{manifests}"
    );
    assert!(
        manifests.contains("- name: isoloom-secrets\n          mountPath: /run/secrets/api-key\n"),
        "{manifests}"
    );
    Ok(())
}
