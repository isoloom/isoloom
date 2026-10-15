//! `constraints:`: a customer's proxy, registry and package mirrors and time servers, applied on
//! every target, and nothing changed without them.

use isoloom_render::constraints::rewrite_image;
use isoloom_spec::model::Constraints;
use isoloom_spec::{Target, load, parse};
use isoloom_targets::{Lab, generate};

use crate::testing::{Outcome, examples, generated as file, valid};

const CONSTRAINTS: &str = "
constraints:
  proxy: { http: \"http://proxy.corp:3128\", no_proxy: [.corp] }
  registries: { docker.io: registry.corp/dockerhub, ghcr.io: registry.corp/ghcr }
  mirrors: { debian: \"http://mirror.corp/debian\", pypi: \"https://pypi.corp/simple\" }
  ntp: [ntp.corp]
";

/// The segmented example (a built web image, a pulled cache image, Isoloom's router and
/// checks), with CONSTRAINTS.
fn constrained() -> Outcome<Lab> {
    let text = std::fs::read_to_string(examples().join("segmented/isoloom.yml"))?;
    valid(&format!("{text}{CONSTRAINTS}"))
}

/// Constraints with these registry mirrors only.
fn mirrors(registries: &[(&str, &str)]) -> Constraints {
    Constraints {
        registries: registries.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
        ..Default::default()
    }
}

#[test]
fn images_go_through_their_registry_mirror() {
    let c = mirrors(&[("docker.io", "registry.corp/dockerhub/"), ("ghcr.io", "registry.corp/ghcr")]);
    assert_eq!(rewrite_image(&c, "redis:7"), "registry.corp/dockerhub/library/redis:7");
    assert_eq!(rewrite_image(&c, "curlimages/curl:8.11.1"), "registry.corp/dockerhub/curlimages/curl:8.11.1");
    assert_eq!(rewrite_image(&c, "docker.io/library/nginx"), "registry.corp/dockerhub/library/nginx");
    assert_eq!(rewrite_image(&c, "index.docker.io/x/y@sha256:ab"), "registry.corp/dockerhub/x/y@sha256:ab");
    assert_eq!(rewrite_image(&c, "ghcr.io/org/app:1"), "registry.corp/ghcr/org/app:1");
    assert_eq!(rewrite_image(&c, "quay.io/org/app"), "quay.io/org/app", "no mirror for quay.io");
    assert_eq!(rewrite_image(&c, "localhost:5000/app"), "localhost:5000/app");
}

#[test]
fn compose_pulls_through_the_mirror_and_machines_get_the_proxy() -> Outcome {
    let compose = file(&constrained()?, Target::Docker, "docker/compose.yml")?;
    assert!(compose.contains("image: registry.corp/dockerhub/library/redis:7-alpine"), "the spec's images");
    assert!(compose.contains("image: registry.corp/dockerhub/library/alpine:3.20"), "Isoloom's own");
    assert!(compose.contains("image: isoloom/segmented-web"), "a built image is local, not pulled");
    assert!(!compose.contains("image: redis:7-alpine") && !compose.contains("image: alpine:3.20"));
    // The built machine: proxy and mirrors as build arguments and environment.
    let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&compose).unwrap();
    let web = &doc["services"]["web"];
    assert_eq!(web["build"]["args"]["HTTP_PROXY"].as_str(), Some("http://proxy.corp:3128"));
    assert_eq!(
        web["environment"]["https_proxy"].as_str(),
        Some("http://proxy.corp:3128"),
        "https defaults to http"
    );
    assert_eq!(web["environment"]["PIP_INDEX_URL"].as_str(), Some("https://pypi.corp/simple"));
    let np = web["environment"]["NO_PROXY"].as_str().unwrap();
    for direct in ["localhost", "cache", "web", "10.61.10.0/24", ".corp"] {
        assert!(np.split(',').any(|x| x == direct), "{direct} reached directly: {np}");
    }
    // Isoloom's checks never go through the customer's proxy.
    for (name, svc) in doc["services"].as_mapping().unwrap() {
        let name = name.as_str().unwrap();
        if name.starts_with("isoloom-") {
            assert!(svc["environment"]["HTTP_PROXY"].is_null(), "{name} has no proxy");
        }
    }
    Ok(())
}

#[test]
fn kubernetes_pulls_through_the_mirror_and_containers_get_the_proxy() -> Outcome {
    let spec = constrained()?;
    let all: String = generate(&spec, Target::Kubernetes).unwrap().into_iter().map(|f| f.contents).collect();
    assert!(all.contains("image: registry.corp/dockerhub/library/redis:7-alpine"));
    assert!(all.contains("image: isoloom/segmented-web"));
    assert!(all.contains("name: HTTP_PROXY") && all.contains("value: http://proxy.corp:3128"), "{all}");
    Ok(())
}

#[test]
fn every_linux_vm_runs_the_preamble_first() -> Outcome {
    let spec = constrained()?;
    let vf = file(&spec, Target::Vagrant, "vagrant/Vagrantfile")?;
    // cache, web, user and the router: each VM's first provisioner.
    assert_eq!(vf.matches("name: \"constraints\"").count(), 4, "{vf}");
    for block in vf.split("config.vm.define").skip(1) {
        let first = block.lines().find(|l| l.contains(".vm.provision")).unwrap();
        assert!(first.contains("name: \"constraints\""), "first provisioner: {first}");
    }
    assert!(vf.contains("Acquire::http::Proxy") && vf.contains("mirror.corp/debian") && vf.contains("ntp.corp") && vf.contains("timesyncd"));

    let pve = file(&spec, Target::Proxmox, "proxmox/main.tf")?;
    assert_eq!(
        pve.matches("\"sh -c 'set -e\\n# The proxy").count(),
        pve.matches("#cloud-config").count(),
        "every cloud-config"
    );

    // On the clouds from the plan, the `Prelude` step comes first, right after cloud-init is
    // done, on every VM.
    for cloud in ["aws", "azure", "gcp"] {
        let tf = file(&spec, Target::CloudVm, &format!("cloud-vm/{cloud}/main.tf"))?;
        assert_eq!(
            tf.matches("\"cloud-init status --wait >/dev/null 2>&1 || true\",\n      \"sudo sh -c 'set -e\\n# The proxy")
                .count(),
            tf.matches("cloud-init status --wait").count(),
            "{cloud}: {tf}"
        );
    }
    for cloud in ["aws", "azure", "gcp", "digitalocean", "linode", "oci"] {
        let tf = file(&spec, Target::CloudDocker, &format!("cloud-docker/{cloud}/main.tf"))?;
        assert_eq!(tf.matches("sudo sh -c 'set -e").count(), 1, "the Docker host on {cloud}");
    }
    // On Proxmox, before cloud-init installs the guest agent (through the proxy), then again
    // over SSH before Docker.
    let pve_docker = file(&spec, Target::DockerVm, "docker-vm/proxmox/main.tf")?;
    assert!(pve_docker.contains("bootcmd = [\n        \"sh -c 'set -e\\n# The proxy"), "{pve_docker}");
    assert_eq!(pve_docker.matches("sudo sh -c 'set -e").count(), 1, "{pve_docker}");
    let docker_vm = file(&spec, Target::DockerVm, "docker-vm/Vagrantfile")?;
    let first = docker_vm.lines().find(|l| l.contains("config.vm.provision")).unwrap();
    assert!(first.contains("name: \"constraints\""), "{first}");
    Ok(())
}

#[test]
fn without_constraints_nothing_changes() {
    let dir = crate::testing::examples().join("segmented");
    let source = load(&dir).unwrap();
    let mut with_empty = source.clone();
    with_empty.constraints = Some(Constraints::default());
    let (plain, empty) = (Lab::new(source), Lab::new(with_empty));
    for t in [Target::Docker, Target::Kubernetes, Target::Vagrant, Target::Proxmox, Target::CloudVm] {
        let a: Vec<_> = generate(&plain, t)
            .unwrap()
            .into_iter()
            .filter(|f| !f.path.ends_with("resolved.json"))
            .collect();
        let b: Vec<_> = generate(&empty, t)
            .unwrap()
            .into_iter()
            .filter(|f| !f.path.ends_with("resolved.json"))
            .collect();
        assert_eq!(a, b, "{t:?}");
    }
}

#[test]
fn constraints_are_validated() {
    let dir = crate::testing::examples().join("segmented");
    let text = std::fs::read_to_string(dir.join("isoloom.yml")).unwrap();
    let lab = Lab::new(
        parse(&format!(
            "{text}
constraints:
  proxy: {{ http: proxy.corp:3128, no_proxy: [\"a, b\"] }}
  registries: {{ \"https://docker.io\": \"https://registry.corp\" }}
  mirrors: {{ npm: registry.npmjs.org }}
  ntp: [\"\"]
"
        ))
        .unwrap(),
    );
    let at: Vec<String> = lab.problems().into_iter().map(|p| p.path.to_string()).collect();
    for expected in [
        "constraints.proxy.http",
        "constraints.proxy.no_proxy[0]",
        "constraints.registries.https://docker.io",
        "constraints.mirrors.npm",
        "constraints.ntp[0]",
    ] {
        assert!(at.iter().any(|a| a == expected), "{expected} in {at:?}");
    }
    let e = parse(&format!("{text}\nconstraints: {{ proxi: {{}} }}\n")).unwrap_err().to_string();
    assert!(e.contains("proxi"), "{e}");
}
