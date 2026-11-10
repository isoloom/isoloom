//! The run manifests the targets write (`.isoloom/<target>/run.json`): the design's web-portal
//! spec and the solo-web example write the shared docker, docker-vm and proxmox fixtures the
//! runner tests against, check runners get the time their waits need, and only `docker`,
//! `vagrant`, `proxmox` and `docker-vm` (two: Vagrant's and Proxmox's) write one. Every example's manifest, on each, parses and matches the schema, and
//! leaves the runner no slot it may skip.

use isoloom_manifest::{Manifest, OutputSource, Part, Placeholder};
use isoloom_spec::{Target, load};
use isoloom_targets::{GeneratedFile, Lab, generate, generate_instance};

use crate::testing::{Outcome, file, lab, matches_fixture};

/// The worked example of the design (run-manifest v0, §9): an input delivered as a file, a set-up
/// job, a published HTTP port, checks with waits, a message pointing at the published port.
const WEB_PORTAL: &str = "
version: 1
name: web-portal
networks:
  lab: { cidr: 10.30.0.0/24, internet: false }
inputs: [DEMO_SECRET]
machines:
  db: { networks: { lab: 20 }, ports: { mysql: 3306 }, app: { image: \"mysql:8.0\" } }
  web:
    networks: { lab: 10 }
    ports: { portal: { port: 80, protocol: http, publish: 8080 } }
    files: [{ path: /srv/demo/secret, input: DEMO_SECRET }]
    volumes: { data: /srv/data }
    depends_on: [db]
    resources: { cpus: 1, memory_mb: 512 }
    setup: [data/secret.sh]
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

/// cloud-docker writes no `published` output in its module (Terraform makes only the host VM);
/// each container's published port is a `published_port` output of the run manifest instead, so
/// the runner reads it back at the host's address.
#[test]
fn cloud_docker_publishes_each_port_through_the_run_manifest() -> Outcome {
    let files = generate(&lab(WEB_PORTAL)?, Target::CloudDocker)?;
    let m = run_json(&files, ".isoloom/cloud-docker/aws/run.json")?;
    let published: Vec<(&str, &str)> = m
        .outputs
        .iter()
        .filter_map(|o| match &o.from {
            OutputSource::PublishedPort { machine, port } => Some((machine.as_str(), port.as_str())),
            OutputSource::TerraformOutput { .. } => None,
        })
        .collect();
    assert_eq!(published, [("web", "portal")], "web's portal is the lab's one published port");
    assert!(
        !file(&files, ".isoloom/cloud-docker/aws/main.tf")?.contains("output \"published\""),
        "the module writes none"
    );
    Ok(())
}

/// cloud-docker lists the lab's networks as docker-vm does, by their Compose names in the VM: the
/// runner reads them for the lab's status. An empty list left a runner's UI drawing networks
/// guessed from the machines' addresses.
#[test]
fn cloud_docker_lists_the_labs_networks() -> Outcome {
    let files = generate(&lab(WEB_PORTAL)?, Target::CloudDocker)?;
    let cloud = run_json(&files, ".isoloom/cloud-docker/aws/run.json")?;
    let local = run_json(&generate(&lab(WEB_PORTAL)?, Target::DockerVm)?, ".isoloom/docker-vm/run.json")?;
    assert!(!cloud.networks.is_empty(), "{:?}", cloud.networks);
    assert_eq!(cloud.networks, local.networks, "the same networks as docker-vm's");
    Ok(())
}

#[test]
fn the_web_portal_spec_writes_the_shared_docker_fixture() -> Outcome {
    matches_fixture(
        run_json(&generate(&lab(WEB_PORTAL)?, Target::Docker)?, ".isoloom/docker/run.json")?,
        "docker.run.json",
    )
}

#[test]
fn the_web_portal_spec_writes_the_shared_docker_vm_fixtures() -> Outcome {
    let files = generate(&lab(WEB_PORTAL)?, Target::DockerVm)?;
    matches_fixture(run_json(&files, ".isoloom/docker-vm/run.json")?, "docker-vm.run.json")?;
    matches_fixture(run_json(&files, ".isoloom/docker-vm/proxmox/run.json")?, "docker-vm-proxmox.run.json")
}

#[test]
fn the_solo_web_example_writes_the_shared_proxmox_fixture() -> Outcome {
    let solo_web = examples(Target::Proxmox)?
        .into_iter()
        .find(|(name, _)| name == "solo-web")
        .ok_or("no solo-web")?
        .1;
    matches_fixture(
        run_json(&generate(&solo_web, Target::Proxmox)?, ".isoloom/proxmox/run.json")?,
        "proxmox.run.json",
    )
}

#[test]
fn check_runners_get_their_waits_plus_15_s_a_check_plus_60_s() -> Outcome {
    let m = run_json(&generate(&lab(WEB_PORTAL)?, Target::Docker).unwrap(), ".isoloom/docker/run.json")?;
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
    let m = run_json(&generate(&lab(WEB_PORTAL)?, Target::Docker).unwrap(), ".isoloom/docker/run.json")?;
    let message = m.environment.message.unwrap();
    assert_eq!(message.to_string(), "The portal answers at http://localhost:${published.web.portal}/.\n");
    assert!(message.parts().iter().any(|p| matches!(p, Part::Placeholder(Placeholder::Published { .. }))));
    Ok(())
}

/// The targets that write a run manifest, where they write it, and how many examples each
/// generates at least.
const WRITERS: [(Target, &str, usize); 5] = [
    (Target::Docker, ".isoloom/docker/run.json", 15),
    (Target::Vagrant, ".isoloom/vagrant/run.json", 10),
    (Target::Proxmox, ".isoloom/proxmox/run.json", 10),
    (Target::DockerVm, ".isoloom/docker-vm/run.json", 10),
    (Target::DockerVm, ".isoloom/docker-vm/proxmox/run.json", 10),
];

/// Every example `target` generates.
fn examples(target: Target) -> Outcome<Vec<(String, Lab)>> {
    let mut out: Vec<(String, Lab)> = Vec::new();
    for entry in std::fs::read_dir(crate::testing::examples())? {
        let path = entry?.path();
        if !path.join("isoloom.yml").is_file() {
            continue;
        }
        let lab = Lab::new(load(&path)?);
        if generate(&lab, target).is_ok() {
            out.push((path.file_name().ok_or("an example with no name")?.to_string_lossy().into_owned(), lab));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Compose refuses `up` and `start` while a secret's variable is unset, and an input's file is
/// such a secret; the runner delivers a VM lab's slot without a value only when the manifest says
/// what to deliver: the manifest never leaves the runner a slot it may skip. A slot it delivers
/// through the environment is required, or says what to deliver when it has no value.
#[test]
fn every_env_delivered_slot_has_a_value_or_a_default() -> Outcome {
    let mut manifests = vec![(
        "web-portal".to_owned(),
        run_json(&generate(&lab(WEB_PORTAL)?, Target::Docker).unwrap(), ".isoloom/docker/run.json")?,
    )];
    for (target, path, _) in WRITERS {
        for (name, spec) in examples(target)? {
            manifests.push((format!("{name} ({target:?})"), run_json(&generate(&spec, target).unwrap(), path)?));
        }
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
    let portal = lab(WEB_PORTAL)?;
    let m = run_json(&generate(&portal, Target::Docker).unwrap(), ".isoloom/docker/run.json")?;
    let slot = m.secrets.iter().find(|s| s.name.to_string() == "DEMO_SECRET").unwrap();
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
        "version: 1\nname: s\nnetworks:\n  lab: { cidr: 10.30.0.0/24 }\ninputs: { API_URL: { sensitive: false }, DEMO_SECRET: {} }\nmachines:\n  web:\n    networks: { lab: 10 }\n    environment: { API_URL: { input: API_URL } }\n    files: [{ path: /srv/secret, input: DEMO_SECRET }]\n    app: { image: nginx }\n",
    )?;
    let m = run_json(&generate(&spec, Target::Docker).unwrap(), ".isoloom/docker/run.json")?;
    let sensitive = |name: &str| m.secrets.iter().find(|s| s.name.to_string() == name).map(|s| s.sensitive);
    assert_eq!(sensitive("API_URL"), Some(false));
    assert_eq!(sensitive("DEMO_SECRET"), Some(true));
    Ok(())
}

#[test]
fn every_examples_manifest_parses_and_matches_the_schema() -> Outcome {
    let schema = jsonschema::validator_for(&isoloom_manifest::schema()).unwrap();
    for (target, path, at_least) in WRITERS {
        let examples = examples(target)?;
        assert!(examples.len() >= at_least, "{target:?}: {}", examples.len());
        for (name, spec) in examples {
            let files = generate(&spec, target).unwrap();
            let file = files.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("{name}: no {path}"));
            isoloom_manifest::parse(&file.contents).unwrap_or_else(|e| panic!("{name} ({target:?}): {e}"));
            let doc: serde_json::Value = serde_json::from_str(&file.contents).unwrap();
            let errors: Vec<String> = schema.iter_errors(&doc).map(|e| format!("{e} at {}", e.instance_path())).collect();
            assert!(errors.is_empty(), "{name} ({target:?}): {errors:#?}");
        }
    }
    Ok(())
}

#[test]
fn only_docker_vagrant_proxmox_and_docker_vm_write_a_run_manifest_for_now() -> Outcome {
    let spec = lab(WEB_PORTAL)?;
    for target in [Target::Hosted, Target::Kubernetes] {
        let files = generate(&spec, target).unwrap_or_default();
        assert!(!files.iter().any(|f| f.path.ends_with("run.json")), "{}", target.id());
    }
    Ok(())
}

#[test]
fn an_instance_writes_its_manifest_in_its_own_folder() -> Outcome {
    let files = generate_instance(&lab(WEB_PORTAL)?, Target::Docker, 2).unwrap();
    let m = run_json(&files, ".isoloom-2/docker/run.json")?;
    assert_eq!(m.environment.instance.map(isoloom_manifest::Instance::get), Some(2));
    assert_eq!(m.environment.name.as_str(), "web-portal-2");
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
    assert_eq!(secret["machines"], serde_json::json!(["app", "isoloom-init-app-1"]));
    assert_eq!(secret["deliver"][0]["var"], "ISOLOOM_SECRET_DB_PASSWORD");
    // To the steps that create or start them (Compose writes the file again at each start).
    assert_eq!(secret["deliver"][0]["steps"], serde_json::json!(["up", "start", "job-isoloom-init-app-1"]));
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
    // An HTTPS port is the entry; a tool's port is information. Both are fixed, so ISOLOOM_PUBLISH_FIXED
    // does nothing here, but it is still declared: an embedder may pass it for any lab that publishes.
    assert_eq!(json["outputs"][0]["format"], "https://127.0.0.1:${value}/");
    assert_eq!(json["outputs"][1]["from"]["machine"], "isoloom-tool-shell");
    let params: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(params, ["ISOLOOM_PUBLISH_ADDRESS", "ISOLOOM_PUBLISH_FIXED"]);
    Ok(())
}

/// A lab with an access machine of its own, checked from its seat: the checks run from a
/// stand-in at its address, started before them. After them, whatever happened, the stand-in is
/// taken down, or the access machine (brought up on demand) finds its address taken.
const ACCESS_SEAT: &str = "
version: 1
name: seat
networks:
  lab: { cidr: 10.9.0.0/24 }
machines:
  web: { networks: { lab: 10 }, ports: { web: { port: 80, protocol: http } }, app: { image: nginx } }
  user: { os: debian@12, access: true, networks: { lab: 20 } }
checks:
  - { name: the page answers from the seat, from: user, http: \"http://web/\" }
";

/// Every container target's run manifest, with where it is written.
const CONTAINER_MANIFESTS: [(Target, &str); 4] = [
    (Target::Docker, ".isoloom/docker/run.json"),
    (Target::DockerVm, ".isoloom/docker-vm/run.json"),
    (Target::DockerVm, ".isoloom/docker-vm/proxmox/run.json"),
    (Target::CloudDocker, ".isoloom/cloud-docker/aws/run.json"),
];

fn ids(steps: &[isoloom_manifest::Step]) -> Vec<String> {
    steps.iter().map(|s| s.id.as_str().to_owned()).collect()
}

#[test]
fn the_check_stand_in_is_taken_down_after_the_checks_on_every_container_target() -> Outcome {
    for (target, path) in CONTAINER_MANIFESTS {
        let m = run_json(&generate(&lab(ACCESS_SEAT)?, target)?, path)?;
        assert_eq!(ids(&m.checks.setup), ["stand-ins"], "{path}");
        assert_eq!(ids(&m.checks.teardown), ["stand-ins-down"], "{path}");
        let down = serde_json::to_string(&m.checks.teardown[0])?;
        assert!(
            down.contains("rm") && down.contains("--stop") && down.contains("isoloom-access"),
            "{path}: {down}"
        );
        // A stand-in that didn't start stops the checks; one left behind fails none of them.
        assert_eq!(m.checks.setup[0].failure, isoloom_manifest::Failure::Fail, "{path}");
        assert_eq!(m.checks.teardown[0].failure, isoloom_manifest::Failure::Tolerate, "{path}");
    }
    Ok(())
}

#[test]
fn a_lab_without_an_access_machine_has_no_stand_in_to_start_or_take_down() -> Outcome {
    let no_seat = ACCESS_SEAT
        .replace("  user: { os: debian@12, access: true, networks: { lab: 20 } }\n", "")
        .replace(", from: user", "");
    for (target, path) in CONTAINER_MANIFESTS {
        let m = run_json(&generate(&lab(&no_seat)?, target)?, path)?;
        assert!(!m.checks.runners.is_empty(), "{path}: the checks still run");
        assert_eq!(ids(&m.checks.setup), Vec::<String>::new(), "{path}");
        assert_eq!(ids(&m.checks.teardown), Vec::<String>::new(), "{path}");
    }
    Ok(())
}

#[test]
fn the_vagrantfiles_checks_run_the_manifests_stand_ins_and_runners() -> Outcome {
    let files = generate(&lab(ACCESS_SEAT)?, Target::DockerVm)?;
    let vf = file(&files, ".isoloom/docker-vm/Vagrantfile")?;
    let manifest = isoloom_manifest::parse(file(&files, ".isoloom/docker-vm/run.json")?)?;
    let checks = vf.split("name: \"checks\"").nth(1).ok_or("no checks provisioner")?;
    let checks = checks.split("\n  config.vm").next().unwrap_or_default();
    let up = at(checks, "--profile check up --detach --wait --no-deps isoloom-access || exit 1")?;
    let down = at(
        checks,
        "--profile check rm --stop --force isoloom-access || echo 'stand-ins-down: failed, tolerated'",
    )?;
    // Each runner of the run manifest, between them, its failure the provisioner's.
    let runners = &manifest.checks.runners;
    assert_eq!(checks.matches(" || failed=1").count(), runners.len(), "{checks}");
    for r in runners {
        let runner = at(checks, &format!("--env ISOLOOM_DERIVED=\\\"$ISOLOOM_DERIVED\\\" {} || failed=1", r.id))?;
        assert!(up < runner && runner < down, "{checks}");
    }
    assert!(
        checks.contains("\nexit \\\"$failed\\\"\", env: "),
        "the runners' verdict is the provisioner's: {checks}"
    );
    Ok(())
}

#[test]
fn the_vagrantfile_tolerates_what_the_manifest_tolerates_and_stops_at_the_rest() -> Outcome {
    let published = "version: 1\nname: pub\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lab: 10 }, app: { image: 'ghcr.io/example/lab-web:1', build: { context: web } } }\n";
    let files = generate(&lab(published)?, Target::DockerVm)?;
    let vf = file(&files, ".isoloom/docker-vm/Vagrantfile")?;
    let start = vf.split("name: \"environment\"").nth(1).ok_or("no environment provisioner")?;
    let pull = at(start, "pull --ignore-pull-failures || echo 'pull: failed, tolerated' >&2")?;
    let up = at(start, "--wait-timeout 600 || exit 1")?;
    assert!(pull < up && up < at(start, "echo ready")?, "{start}");
    Ok(())
}

/// Where `what` is in `text`.
fn at(text: &str, what: &str) -> Outcome<usize> {
    Ok(text.find(what).ok_or_else(|| format!("no {what} in {text}"))?)
}

/// One VM-or-container machine reading a secret and an input as files and another input in its
/// environment: every target that runs it gives each the same slot.
const VALUES: &str = "
version: 1
name: values
networks: { lan: { cidr: 10.72.0.0/24 } }
secrets:
  db-password: { env: DB_PASSWORD }
inputs:
  LAUNCH_TOKEN: {}
  API_URL: { sensitive: false }
machines:
  db:
    os: debian@12
    networks: { lan: 10 }
    environment: { API_URL: { input: API_URL } }
    files:
      - { path: /run/db-password, secret: db-password }
      - { path: /run/launch-token, input: LAUNCH_TOKEN }
    app: { image: nginx, steps: [{ run: p.sh }] }
";

#[test]
fn a_secret_and_an_input_get_the_same_slot_on_every_target() -> Outcome {
    let lab = lab(VALUES)?;
    // What a slot says whatever the target: its name, kind, whether it must be given, whether it
    // is masked, where the runner reads it, its value when left out (its deliveries differ).
    let shape = |m: &Manifest| -> Vec<String> {
        let mut out: Vec<String> = m
            .secrets
            .iter()
            .filter(|s| s.kind != isoloom_manifest::SlotKind::Connection)
            .map(|s| {
                format!(
                    "{} {:?} required={} sensitive={} source={:?} default={:?}",
                    s.name, s.kind, s.required, s.sensitive, s.source, s.default
                )
            })
            .collect();
        out.sort();
        out
    };
    let docker = shape(&run_json(&generate(&lab, Target::Docker)?, ".isoloom/docker/run.json")?);
    assert_eq!(docker.len(), 3, "{docker:?}");
    for (target, path) in [
        (Target::Vagrant, ".isoloom/vagrant/run.json"),
        (Target::CloudVm, ".isoloom/cloud-vm/aws/run.json"),
        (Target::CloudVm, ".isoloom/cloud-vm/digitalocean/run.json"),
        (Target::CloudDocker, ".isoloom/cloud-docker/gcp/run.json"),
    ] {
        assert_eq!(shape(&run_json(&generate(&lab, target)?, path)?), docker, "{path}");
    }
    Ok(())
}

#[test]
fn proxmox_refuses_values_as_files_it_could_never_deliver() -> Outcome {
    // Its machines sat waiting half an hour for a delivery the run manifest had no slot for.
    let why = isoloom_targets::refusal(&lab(VALUES)?, Target::Proxmox).ok_or("proxmox takes the lab")?;
    assert!(why.contains("machine `db` gets `secrets`") && why.contains("nothing delivers them"), "{why}");
    // An input in a machine's environment still goes in through the module's `inputs`.
    let env_only = VALUES
        .replace(
            "      - { path: /run/db-password, secret: db-password }\n      - { path: /run/launch-token, input: LAUNCH_TOKEN }\n",
            "",
        )
        .replace("    files:\n", "");
    let m = run_json(&generate(&lab(&env_only)?, Target::Proxmox)?, ".isoloom/proxmox/run.json")?;
    assert!(m.secrets.iter().any(|s| s.name.as_str() == "API_URL"), "{:?}", m.secrets);
    Ok(())
}

#[test]
fn checks_run_from_a_linux_vm_or_the_controller_on_proxmox_as_on_the_clouds() -> Outcome {
    // A Windows machine has no `sh` to pipe its checks into: on Proxmox they ran there.
    let lab = isoloom_targets::Lab::new(load(&crate::testing::examples().join("mixed-office"))?);
    let from = |m: &Manifest| -> Vec<(String, String)> { m.checks.runners.iter().map(|r| (r.id.to_string(), r.from.clone())).collect() };
    let pve = from(&run_json(&generate(&lab, Target::Proxmox)?, ".isoloom/proxmox/run.json")?);
    let azure = from(&run_json(&generate(&lab, Target::CloudVm)?, ".isoloom/cloud-vm/azure/run.json")?);
    assert_eq!(pve, azure);
    assert!(pve.iter().all(|(_, f)| f != "from files01"), "{pve:?}");
    // With no Linux machine, the controller, named alike.
    let windows = isoloom_targets::Lab::new(load(&crate::testing::examples().join("windows-hello"))?);
    let only_windows = from(&run_json(&generate(&windows, Target::Proxmox)?, ".isoloom/proxmox/run.json")?);
    assert!(
        !only_windows.is_empty() && only_windows.iter().all(|(_, f)| f == "from isoloom-controller"),
        "{only_windows:?}"
    );
    Ok(())
}

#[test]
fn a_machines_size_follows_one_rule_on_every_target_and_in_the_snapshot() -> Outcome {
    // A machine that is a container or a VM, and says nothing of its size.
    let lab = lab(
        "version: 1\nname: sized\nnetworks: { lan: { cidr: 10.73.0.0/24 } }\nmachines:\n  web: { os: debian@12, networks: { lan: 10 }, app: { image: nginx, steps: [{ run: p.sh }] } }\n",
    )?;
    let size = |target: Target, path: &str| -> Outcome<(Option<u32>, Option<u32>)> {
        let m = run_json(&generate(&lab, target)?, path)?;
        let web = m.machines.iter().find(|x| x.name.as_str() == "web").ok_or("no web")?;
        let r = web.resources.ok_or("no resources")?;
        Ok((r.cpus, r.memory_mb))
    };
    let container = isoloom_spec::MachineSize::of(None, true);
    let vm = isoloom_spec::MachineSize::of(None, false);
    assert_eq!(
        size(Target::Docker, ".isoloom/docker/run.json")?,
        (Some(container.cpus), Some(container.memory_mb))
    );
    for (target, path) in [
        (Target::Vagrant, ".isoloom/vagrant/run.json"),
        (Target::Proxmox, ".isoloom/proxmox/run.json"),
        (Target::CloudVm, ".isoloom/cloud-vm/aws/run.json"),
    ] {
        assert_eq!(size(target, path)?, (Some(vm.cpus), Some(vm.memory_mb)), "{path}");
    }
    // The snapshot gives its size in each form, as the totals count them.
    let snapshot = isoloom_targets::resolved::resolve(&lab);
    let of = |form: &str| {
        snapshot
            .pointer(&format!("/machines/web/resources/{form}/memory_mb"))
            .and_then(serde_json::Value::as_u64)
    };
    assert_eq!((of("docker"), of("vm")), (Some(u64::from(container.memory_mb)), Some(u64::from(vm.memory_mb))));
    assert_eq!(
        isoloom_spec::totals_for(&lab.for_target(Target::Docker), Target::Docker).memory_mb,
        container.memory_mb
    );
    Ok(())
}

/// Each Terraform module asks for the Terraform its run manifest names: 1.6, a cloud's as a
/// server's (nothing they write is newer).
#[test]
fn a_module_requires_the_terraform_its_run_manifest_names() -> Outcome {
    for target in [Target::CloudDocker, Target::CloudVm, Target::Proxmox] {
        let oldest = "1.6.0";
        for (name, spec) in examples(target)?.into_iter().take(3) {
            let files = generate(&spec, target)?;
            let manifests: Vec<&GeneratedFile> = files.iter().filter(|f| f.path.ends_with("/run.json")).collect();
            assert!(!manifests.is_empty(), "{name} ({target:?}): no run manifest");
            for file in manifests {
                let manifest = isoloom_manifest::parse(&file.contents)?;
                let terraform = manifest.tools.iter().find(|t| t.name.as_str() == "terraform").ok_or("no terraform tool")?;
                assert_eq!(
                    terraform.min_version.as_ref().map(isoloom_manifest::Version::as_str),
                    Some(oldest),
                    "{}",
                    file.path
                );
                let module = file.path.replace("run.json", "main.tf");
                assert!(
                    files
                        .iter()
                        .any(|f| f.path == module && f.contents.contains(&format!("required_version = \">= {oldest}\""))),
                    "{module} doesn't require Terraform {oldest}"
                );
            }
        }
    }
    Ok(())
}
