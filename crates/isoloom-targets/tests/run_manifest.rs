//! The run manifest the `docker` target writes (`.isoloom/docker/run.json`): the design's
//! flag-portal spec writes the shared fixture the runner tests against, every example's manifest
//! parses and matches the schema, check runners get the time their waits need, and only `docker`
//! writes one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::{Path, PathBuf};

use isoloom_manifest::{Manifest, Part, Placeholder, Version};
use isoloom_spec::{Target, load, parse};
use isoloom_targets::{Environment, GeneratedFile, generate, generate_instance};

/// The worked example of the design (run-manifest v0, §9): an init job filling a volume with an
/// input, a published HTTP port, checks with waits, a message pointing at the published port.
const FLAG_PORTAL: &str = "
version: 1
name: flag-portal
networks:
  lab: { cidr: 10.30.0.0/24, internet: false }
inputs: [CTF_FLAG_MAIN]
machines:
  db:
    networks: { lab: 20 }
    ports: [{ port: 3306, name: mysql }]
    docker: { image: mysql:8.0 }
  web:
    networks: { lab: 10 }
    ports: [{ port: 80, name: portal, http: true, publish: 8080 }]
    inputs: [CTF_FLAG_MAIN]
    volumes: { ctf: /ctf }
    depends_on: [db]
    resources: { cpus: 1, memory_mb: 512 }
    docker:
      build: build/web
      init: [ctf/flag.sh]
checks:
  - { http: \"http://web/\", wait: 60 }
  - { tcp: \"db:3306\", wait: 120 }
  - { script: build/check/check.sh, wait: 180 }
message: |
  The portal answers at http://localhost:{{ machines.web.ports.0.publish }}/.
";

fn run_json(files: &[GeneratedFile], path: &str) -> Manifest {
    let file = files.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("no {path}"));
    isoloom_manifest::parse(&file.contents).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../isoloom-manifest/fixtures").join(name)
}

#[test]
fn the_flag_portal_spec_writes_the_shared_docker_fixture() {
    let mut generated = run_json(&generate(&parse(FLAG_PORTAL).unwrap(), Target::Docker).unwrap(), ".isoloom/docker/run.json");
    let mut fixture = isoloom_manifest::parse(&std::fs::read_to_string(fixture("docker.run.json")).unwrap()).unwrap();
    // A release changes the version, not the contract.
    generated.generator.version = Version::parse("0.0.0").unwrap();
    fixture.generator.version = Version::parse("0.0.0").unwrap();
    assert_eq!(generated, fixture);
}

#[test]
fn check_runners_get_their_waits_plus_15_s_a_check_plus_60_s() {
    let m = run_json(&generate(&parse(FLAG_PORTAL).unwrap(), Target::Docker).unwrap(), ".isoloom/docker/run.json");
    let timeouts: Vec<(String, u32, u32)> = m
        .checks
        .runners
        .iter()
        .map(|r| {
            let waits: u32 = r.checks.iter().map(|c| c.wait_s).sum();
            let n = u32::try_from(r.checks.len()).unwrap();
            (r.id.to_string(), r.step.timeout_s.unwrap().get(), waits + 15 * n + 60)
        })
        .collect();
    assert_eq!(timeouts.len(), 3);
    assert_eq!(timeouts[0], ("isoloom-check".to_string(), 465, 465));
    for (id, timeout, rule) in timeouts {
        assert_eq!(timeout, rule, "{id}");
    }
}

#[test]
fn the_message_leaves_published_ports_to_the_runner() {
    let m = run_json(&generate(&parse(FLAG_PORTAL).unwrap(), Target::Docker).unwrap(), ".isoloom/docker/run.json");
    let message = m.lab.message.unwrap();
    assert_eq!(message.to_string(), "The portal answers at http://localhost:${published.web.80}/.\n");
    assert!(message.parts().iter().any(|p| matches!(p, Part::Placeholder(Placeholder::Published { .. }))));
}

/// Every example the docker target generates.
fn docker_examples() -> Vec<(String, isoloom_spec::Spec)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut out: Vec<(String, isoloom_spec::Spec)> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("isoloom.yml").is_file())
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), load(&p).unwrap()))
        .filter(|(_, spec)| generate(spec, Target::Docker).is_ok())
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn every_examples_manifest_parses_and_matches_the_schema() {
    let schema = jsonschema::validator_for(&isoloom_manifest::schema()).unwrap();
    let examples = docker_examples();
    assert!(examples.len() >= 15, "{}", examples.len());
    for (name, spec) in examples {
        let files = generate(&spec, Target::Docker).unwrap();
        let file = files
            .iter()
            .find(|f| f.path == ".isoloom/docker/run.json")
            .unwrap_or_else(|| panic!("{name}: no run.json"));
        isoloom_manifest::parse(&file.contents).unwrap_or_else(|e| panic!("{name}: {e}"));
        let doc: serde_json::Value = serde_json::from_str(&file.contents).unwrap();
        let errors: Vec<String> = schema.iter_errors(&doc).map(|e| format!("{e} at {}", e.instance_path())).collect();
        assert!(errors.is_empty(), "{name}: {errors:#?}");
    }
}

#[test]
fn the_start_plan_is_computed_as_embedders_read_it_from_the_file() {
    for (name, spec) in docker_examples() {
        let env = Environment::build(&spec, Target::Docker, None);
        let rendered = isoloom_compose::compose_file(Target::Docker, &env).unwrap();
        let yaml = isoloom_compose::to_yaml(Target::Docker, &rendered.file).unwrap();
        assert_eq!(
            isoloom_compose::StartPlan::of(&rendered.file),
            isoloom_compose::start_plan(&yaml).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn only_docker_writes_a_run_manifest_for_now() {
    let spec = parse(FLAG_PORTAL).unwrap();
    for target in [Target::Hosted, Target::DockerVm, Target::Kubernetes, Target::Vagrant] {
        let files = generate(&spec, target).unwrap_or_default();
        assert!(!files.iter().any(|f| f.path.ends_with("run.json")), "{}", target.id());
    }
}

#[test]
fn an_instance_writes_its_manifest_in_its_own_folder() {
    let files = generate_instance(&parse(FLAG_PORTAL).unwrap(), Target::Docker, 2).unwrap();
    let m = run_json(&files, ".isoloom-2/docker/run.json");
    assert_eq!(m.lab.instance.map(isoloom_manifest::Instance::get), Some(2));
    assert_eq!(m.lab.name.as_str(), "flag-portal-2");
}

/// A secret, a project file with an owner, a kept volume and a tool with a published port.
const STORE: &str = "
version: 1
name: store
networks: { lan: { cidr: 10.70.0.0/24 } }
secrets:
  db-password: { env: STORE_DB_PASSWORD }
volumes: { data: { owner: app, persistent: true } }
machines:
  app:
    networks: { lan: 10 }
    ports: [{ port: 443, name: site, http: true, tls: true, publish: 8443, fixed: true }]
    docker: { image: nginx:alpine, init: [seed.sh] }
    secrets: [db-password]
    volumes: { data: /data, cache: /cache }
    files:
      - { path: /etc/app/key.pem, source: conf/key.pem, uid: 101, mode: '0400' }
tools:
  shell: { port: 8080, publish: 18080 }
";

#[test]
fn secrets_project_files_kept_volumes_and_tools_reach_the_manifest() {
    let m = run_json(&generate(&parse(STORE).unwrap(), Target::Docker).unwrap(), ".isoloom/docker/run.json");
    let json = serde_json::to_value(&m).unwrap();
    // The secret goes to the containers that mount it, by its Compose variable, from its store.
    let secret = &json["secrets"][0];
    assert_eq!(secret["name"], "db-password");
    assert_eq!(secret["source"], serde_json::json!({ "store": "env", "reference": "STORE_DB_PASSWORD" }));
    assert_eq!(secret["machines"], serde_json::json!(["app", "app-init-1"]));
    assert_eq!(secret["deliver"][0]["var"], "ISOLOOM_SECRET_DB_PASSWORD");
    assert_eq!(secret["deliver"][0]["steps"], serde_json::json!(["up", "job-app-init-1"]));
    // A project file with an owner travels through Compose's environment too, read from the project.
    assert_eq!(
        json["secrets"][1]["source"],
        serde_json::json!({ "store": "file", "reference": "conf/key.pem" })
    );
    // A kept volume survives `down`; the others are removed by name.
    let down = &json["lifecycle"]["down"];
    assert!(!down[0]["argv"].as_array().unwrap().contains(&"--volumes".into()), "{down:#}");
    assert_eq!(
        down[1]["argv"],
        serde_json::json!(["docker", "volume", "rm", "--force", "${run.name}_app-cache"])
    );
    // An HTTPS port is the entry; a tool's port is information; both fixed, so no ISOLOOM_PUBLISH_FIXED.
    assert_eq!(json["outputs"][0]["format"], "https://127.0.0.1:${value}/");
    assert_eq!(json["outputs"][1]["from"]["machine"], "isoloom-tool-shell");
    let params: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(params, ["ISOLOOM_PUBLISH_ADDRESS"]);
}
