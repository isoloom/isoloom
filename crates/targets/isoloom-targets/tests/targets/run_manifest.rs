//! The run manifest the `docker` target writes (`.isoloom/docker/run.json`): the design's
//! flag-portal spec writes the shared fixture the runner tests against, every example's manifest
//! parses and matches the schema, check runners get the time their waits need, and only `docker`
//! writes one.

use std::path::{Path, PathBuf};

use isoloom_manifest::{Manifest, Part, Placeholder, Version};
use isoloom_spec::{Target, load};
use isoloom_targets::{GeneratedFile, Lab, generate, generate_instance};

use crate::testing::{Outcome, file, lab};

/// The worked example of the design (run-manifest v0, §9): an input delivered as a file, a set-up
/// job, a published HTTP port, checks with waits, a message pointing at the published port.
const FLAG_PORTAL: &str = "
version: 1
name: flag-portal
networks:
  lab: { cidr: 10.30.0.0/24, internet: false }
inputs: [CTF_FLAG_MAIN]
machines:
  db: { networks: { lab: 20 }, ports: { mysql: 3306 }, app: { image: \"mysql:8.0\" } }
  web:
    networks: { lab: 10 }
    ports: { portal: { port: 80, protocol: http, publish: 8080 } }
    files: [{ path: /ctf/flag, input: CTF_FLAG_MAIN }]
    volumes: { ctf: /srv/ctf }
    depends_on: [db]
    resources: { cpus: 1, memory_mb: 512 }
    setup: [ctf/flag.sh]
    app: { build: build/web }
checks:
  - { http: \"http://web/\", wait: 60 }
  - { tcp: \"db:3306\", wait: 120 }
  - { script: build/check/check.sh, wait: 180 }
message: \"The portal answers at http://localhost:{{ machines.web.ports.portal.published }}/.\\n\"
";

/// The run manifest at `path` among `files`, parsed.
fn run_json(files: &[GeneratedFile], path: &str) -> Outcome<Manifest> {
    Ok(isoloom_manifest::parse(file(files, path)?)?)
}

/// The shared run-manifest fixture `name`.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core/isoloom-manifest/fixtures").join(name)
}

#[test]
fn the_flag_portal_spec_writes_the_shared_docker_fixture() -> Outcome {
    let mut generated = run_json(&generate(&lab(FLAG_PORTAL)?, Target::Docker).unwrap(), ".isoloom/docker/run.json")?;
    let mut fixture = isoloom_manifest::parse(&std::fs::read_to_string(fixture("docker.run.json")).unwrap()).unwrap();
    // A release changes the version, not the contract.
    generated.generator.version = Version::parse("0.0.0").unwrap();
    fixture.generator.version = Version::parse("0.0.0").unwrap();
    assert_eq!(generated, fixture);
    Ok(())
}

#[test]
fn check_runners_get_their_waits_plus_15_s_a_check_plus_60_s() -> Outcome {
    let m = run_json(&generate(&lab(FLAG_PORTAL)?, Target::Docker).unwrap(), ".isoloom/docker/run.json")?;
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
    assert_eq!(timeouts[0], ("isoloom-check".to_owned(), 465, 465));
    for (id, timeout, rule) in timeouts {
        assert_eq!(timeout, rule, "{id}");
    }
    Ok(())
}

#[test]
fn the_message_leaves_published_ports_to_the_runner() -> Outcome {
    let m = run_json(&generate(&lab(FLAG_PORTAL)?, Target::Docker).unwrap(), ".isoloom/docker/run.json")?;
    let message = m.lab.message.unwrap();
    assert_eq!(message.to_string(), "The portal answers at http://localhost:${published.web.portal}/.\n");
    assert!(message.parts().iter().any(|p| matches!(p, Part::Placeholder(Placeholder::Published { .. }))));
    Ok(())
}

/// Every example the docker target generates.
fn docker_examples() -> Outcome<Vec<(String, Lab)>> {
    let mut out: Vec<(String, Lab)> = Vec::new();
    for entry in std::fs::read_dir(crate::testing::examples())? {
        let path = entry?.path();
        if !path.join("isoloom.yml").is_file() {
            continue;
        }
        let lab = Lab::new(load(&path)?);
        if generate(&lab, Target::Docker).is_ok() {
            out.push((path.file_name().ok_or("an example with no name")?.to_string_lossy().into_owned(), lab));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Compose refuses `up` and `start` while a secret's variable is unset, and an input's file is
/// such a secret: the manifest never leaves the runner a slot it may skip. A slot it delivers
/// through the environment is required, or says what to deliver when it has no value.
#[test]
fn every_env_delivered_slot_has_a_value_or_a_default() -> Outcome {
    let mut manifests = vec![(
        "flag-portal".to_owned(),
        run_json(&generate(&lab(FLAG_PORTAL)?, Target::Docker).unwrap(), ".isoloom/docker/run.json")?,
    )];
    for (name, spec) in docker_examples()? {
        manifests.push((name, run_json(&generate(&spec, Target::Docker).unwrap(), ".isoloom/docker/run.json")?));
    }
    for (name, m) in manifests {
        for slot in &m.secrets {
            let by_env = slot.deliver.iter().any(|d| matches!(d, isoloom_manifest::Delivery::Env { .. }));
            assert!(
                !by_env || slot.required || slot.default.is_some(),
                "{name}: slot {} may be left unset",
                slot.name
            );
        }
    }
    let flag = lab(FLAG_PORTAL)?;
    let m = run_json(&generate(&flag, Target::Docker).unwrap(), ".isoloom/docker/run.json")?;
    let slot = m.secrets.iter().find(|s| s.name.to_string() == "CTF_FLAG_MAIN").unwrap();
    assert_eq!(
        (slot.required, slot.default.as_deref()),
        (false, Some("")),
        "an input left out is delivered empty"
    );
    Ok(())
}

/// An input's own `sensitive` reaches its slot: one the spec marks `false` is shown, one it says
/// nothing of is masked.
#[test]
fn an_input_keeps_the_sensitivity_the_spec_gives_it() -> Outcome {
    let spec = lab(
        "version: 1\nname: s\nnetworks:\n  lab: { cidr: 10.30.0.0/24 }\ninputs: { API_URL: { sensitive: false }, FLAG: {} }\nmachines:\n  web:\n    networks: { lab: 10 }\n    environment: { API_URL: { input: API_URL } }\n    files: [{ path: /srv/flag, input: FLAG }]\n    app: { image: nginx }\n",
    )?;
    let m = run_json(&generate(&spec, Target::Docker).unwrap(), ".isoloom/docker/run.json")?;
    let sensitive = |name: &str| m.secrets.iter().find(|s| s.name.to_string() == name).map(|s| s.sensitive);
    assert_eq!(sensitive("API_URL"), Some(false));
    assert_eq!(sensitive("FLAG"), Some(true));
    Ok(())
}

#[test]
fn every_examples_manifest_parses_and_matches_the_schema() -> Outcome {
    let schema = jsonschema::validator_for(&isoloom_manifest::schema()).unwrap();
    let examples = docker_examples()?;
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
    Ok(())
}

#[test]
fn only_docker_writes_a_run_manifest_for_now() -> Outcome {
    let spec = lab(FLAG_PORTAL)?;
    for target in [Target::Hosted, Target::DockerVm, Target::Kubernetes, Target::Vagrant] {
        let files = generate(&spec, target).unwrap_or_default();
        assert!(!files.iter().any(|f| f.path.ends_with("run.json")), "{}", target.id());
    }
    Ok(())
}

#[test]
fn an_instance_writes_its_manifest_in_its_own_folder() -> Outcome {
    let files = generate_instance(&lab(FLAG_PORTAL)?, Target::Docker, 2).unwrap();
    let m = run_json(&files, ".isoloom-2/docker/run.json")?;
    assert_eq!(m.lab.instance.map(isoloom_manifest::Instance::get), Some(2));
    assert_eq!(m.lab.name.as_str(), "flag-portal-2");
    Ok(())
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
    ports: { site: { port: 443, protocol: https, publish: { host: 8443, fixed: true } } }
    app: { image: nginx:alpine }
    setup: [seed.sh]
    volumes: { data: /data, cache: /cache }
    files:
      - { path: /run/secrets/db-password, secret: db-password }
      - { path: /etc/app/key.pem, source: conf/key.pem, uid: 101, mode: '0400' }
tools:
  shell: { port: 8080, publish: 18080 }
";

#[test]
fn secrets_project_files_kept_volumes_and_tools_reach_the_manifest() -> Outcome {
    let m = run_json(&generate(&lab(STORE)?, Target::Docker).unwrap(), ".isoloom/docker/run.json")?;
    let json = serde_json::to_value(&m).unwrap();
    // The secret goes to the containers that mount it, by its Compose variable, from its store.
    let secret = &json["secrets"][0];
    assert_eq!(secret["name"], "db-password");
    assert_eq!(secret["source"], serde_json::json!({ "store": "env", "reference": "STORE_DB_PASSWORD" }));
    assert_eq!(secret["machines"], serde_json::json!(["app", "app-init-1"]));
    assert_eq!(secret["deliver"][0]["var"], "ISOLOOM_SECRET_DB_PASSWORD");
    // To the steps that create or start them (Compose writes the file again at each start).
    assert_eq!(secret["deliver"][0]["steps"], serde_json::json!(["up", "start", "job-app-init-1"]));
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
    Ok(())
}
