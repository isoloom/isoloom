//! The `cloud-docker` modules on small plans: the ports on every cloud, the set-up in order, a
//! module per cloud, the refusals.

use isoloom_plan::Environment;
use isoloom_spec::model::Target;

use super::{Cloud, Host, module as cloud_module, refusal};
use crate::testing::environment;

/// Two containers, publishing TCP and UDP ports.
pub(crate) const SPEC: &str = "
version: 1
name: dock
inputs: [TOKEN]
networks:
  lab: { cidr: 10.9.0.0/24 }
machines:
  web:
    networks: { lab: 10 }
    ports: { web: { port: 80, publish: 8080 }, dns: { port: 53, protocol: udp, publish: 5353 } }
    resources: { cpus: 2, memory_mb: 4096, disk_gb: 30 }
    app: { image: nginx }
  db:
    networks: { lab: 11 }
    ports: { db: { port: 5432, publish: 15432 } }
    app: { image: postgres }
";

/// A plan of `yaml` on `cloud-docker`.
pub(crate) fn env(yaml: &str) -> Environment {
    environment(yaml, Target::CloudDocker)
}

/// What the `docker` target gives the host for [`SPEC`]: its Compose file's folder, its start,
/// and the input its containers read.
fn host() -> Host {
    Host {
        outputs: vec![".isoloom/docker".into()],
        start: vec!["cd /opt/isoloom && docker compose --file .isoloom/docker/compose.yml up -d --wait".into()],
        values: vec!["TOKEN".into()],
    }
}

fn modules(e: &Environment) -> crate::Result<isoloom_render::backend::Files> {
    super::modules(e, &host())
}

fn module(cloud: Cloud) -> String {
    cloud_module(cloud, &env(SPEC), &host()).unwrap().contents
}

#[test]
fn every_cloud_opens_ssh_and_the_published_ports() {
    let values = |cloud: Cloud, key: &str| isoloom_test_support::hcl_values(&isoloom_test_support::hcl_body(&module(cloud)), key);
    assert!(values(Cloud::Aws, "description").contains(&"published 5353/udp".to_owned()));
    assert!(
        values(Cloud::Azure, "destination_port_ranges").windows(3).any(|w| w == ["22", "8080", "15432"]),
        "{:?}",
        values(Cloud::Azure, "destination_port_ranges")
    );
    assert!(values(Cloud::Gcp, "ports").contains(&"5353".to_owned()));
    let droplet = isoloom_test_support::hcl_body(&module(Cloud::DigitalOcean));
    let inbound: usize = isoloom_test_support::hcl_blocks(&droplet, "resource", &["digitalocean_firewall"])
        .iter()
        .map(|f| isoloom_test_support::hcl_nested(f.body(), "inbound_rule").len())
        .sum();
    assert_eq!(inbound, 4);
    assert!(values(Cloud::Linode, "ports").contains(&"22,8080,15432".to_owned()));
    assert!(values(Cloud::Oci, "protocol").contains(&"17".to_owned()), "UDP");
}

#[test]
fn the_host_sets_itself_up_and_the_module_waits_for_it() {
    use isoloom_test_support::{hcl_blocks, hcl_body, hcl_names, hcl_nested, hcl_one};
    for cloud in Cloud::ALL {
        let tf = hcl_body(&module(cloud));
        // Terraform brings the project and the Compose file's folder in, then waits for the set-up,
        // signing in with the operator's key.
        assert_eq!(hcl_names(&tf, "resource", "local_file"), ["isoloom_project"], "{cloud:?}");
        assert_eq!(hcl_blocks(&tf, "variable", &["ssh_private_key_file"]).len(), 1, "{cloud:?}");
        let wait = hcl_one(&tf, "terraform_data", "isoloom_ready_docker_host");
        let steps: Vec<(String, String)> = hcl_nested(wait, "provisioner")
            .iter()
            .map(|p| {
                let kind = p.labels().first().map(|l| l.as_str().to_owned()).unwrap_or_default();
                let values = isoloom_test_support::hcl_literals(p.body()).join("\n");
                (kind, values)
            })
            .collect();
        let step = |kind: &str, what: &str| steps.iter().position(|(k, v)| k == kind && v.contains(what));
        let archived = step("local-exec", ".isoloom/docker").unwrap_or_else(|| panic!("{cloud:?}: {steps:?}"));
        let uploaded = step("file", "/.isoloom-outputs-docker_host.tgz").unwrap_or_else(|| panic!("{cloud:?}: {steps:?}"));
        let unpacked = step("remote-exec", "tar -xzf /tmp/isoloom-outputs.tgz -C /opt/isoloom").unwrap_or_else(|| panic!("{cloud:?}: {steps:?}"));
        let waited = step("remote-exec", "cloud-init status --wait").unwrap_or_else(|| panic!("{cloud:?}: {steps:?}"));
        assert!(archived < uploaded && uploaded <= unpacked && unpacked <= waited, "{cloud:?}: {steps:?}");
    }
    // The input is a sensitive variable of its own name, handed to the start in base64.
    let tf = module(Cloud::Aws);
    let body = isoloom_test_support::hcl_body(&tf);
    let token = isoloom_test_support::hcl_blocks(&body, "variable", &["TOKEN"]);
    assert_eq!(
        token.first().map(|v| isoloom_test_support::hcl_text(v.body(), "sensitive")).as_deref(),
        Some("true"),
        "{tf}"
    );
    assert!(isoloom_test_support::hcl_blocks(&body, "variable", &["inputs"]).is_empty(), "{tf}");
    let set_up = super::Host::set_up(&host(), &env(SPEC));
    let text: String = crate::machine::user_data::script(&set_up)
        .pieces()
        .map(|p| match p {
            crate::hcl_builder::Piece::Text(t) => t.to_owned(),
            crate::hcl_builder::Piece::Expr(_) => "<expr>".to_owned(),
        })
        .collect();
    let docker = text.find("running: Docker").unwrap();
    let start = text
        .find("TOKEN=\"$(printf %s '<expr>' | base64 -d)\" sh -c 'cd /opt/isoloom && docker compose")
        .unwrap();
    assert!(text.find("project-delivered").unwrap() < docker && docker < start, "{text}");
    assert!(
        text.contains("signed-by=/etc/apt/keyrings/docker.asc"),
        "Docker from its pinned repository: {text}"
    );
}

/// The expression of `key` in `body`: an attribute of its own or of a nested block, or an
/// object's key (`metadata = { user_data = ... }`).
fn value_of<'b>(body: &'b hcl::Body, key: &str) -> Option<&'b hcl::Expression> {
    body.iter().find_map(|s| match s {
        hcl::Structure::Attribute(a) if a.key() == key => Some(a.expr()),
        hcl::Structure::Attribute(a) => match a.expr() {
            hcl::Expression::Object(o) => o.iter().find(|(k, _)| k.to_string().trim_matches('"') == key).map(|(_, v)| v),
            _ => None,
        },
        hcl::Structure::Block(b) => value_of(b.body(), key),
    })
}

#[test]
fn each_cloud_hands_the_set_up_to_its_own_first_boot() {
    let cases = [
        (Cloud::Aws, "aws_instance", "user_data_base64", Some("base64gzip")),
        (Cloud::Azure, "azurerm_linux_virtual_machine", "custom_data", Some("base64encode")),
        (Cloud::Gcp, "google_compute_instance", "startup-script", None),
        (Cloud::DigitalOcean, "digitalocean_droplet", "user_data", None),
        (Cloud::Linode, "linode_instance", "user_data", Some("base64encode")),
        (Cloud::Oci, "oci_core_instance", "user_data", Some("base64encode")),
    ];
    for (cloud, kind, key, wrap) in cases {
        let host = isoloom_test_support::hcl_resource(&module(cloud), kind, "env");
        let value = value_of(&host, key).unwrap_or_else(|| panic!("{cloud:?}: no {key}"));
        // The script itself, or the one argument of the encoding the cloud takes.
        let script = match (wrap, value) {
            (Some(f), hcl::Expression::FuncCall(call)) if call.name.name.as_str() == f => call.args.first(),
            (None, script) => Some(script),
            _ => None,
        };
        let text = script.map(ToString::to_string).unwrap_or_default();
        assert!(text.trim_start_matches('"').starts_with("#!/bin/sh"), "{cloud:?}: {key} = {value}");
    }
}

#[test]
fn arm64_is_refused_from_the_plan() {
    let e = env(
        "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    hardware: { arch: arm64 }\n    app: { image: nginx }\n",
    );
    let why = refusal(&e).unwrap();
    assert_eq!(
        why,
        "machine `w`: arm64, and every cloud's Docker host is x86-64 (its module picks x86-64 sizes and images)"
    );
    assert!(modules(&e).unwrap_err().to_string().ends_with(&why));
}

#[test]
fn a_module_per_cloud() {
    let files = modules(&env(SPEC)).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    for cloud in Cloud::ALL {
        assert!(paths.contains(&format!(".isoloom/cloud-docker/{}/main.tf", cloud.id()).as_str()), "{paths:?}");
    }
}

/// One container asking for `disk_gb` of disk and `memory_mb` of memory.
fn one_container(memory_mb: u32, disk_gb: u32) -> Environment {
    env(&format!(
        "version: 1\nname: big\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  w:\n    networks: {{ lab: 10 }}\n    resources: {{ cpus: 1, memory_mb: {memory_mb}, disk_gb: {disk_gb} }}\n    app: {{ image: nginx }}\n"
    ))
}

#[test]
fn a_cloud_whose_size_is_its_disk_too_sizes_the_host_by_its_table() {
    // 100 GB of containers (and Docker's own): past the 80 GB of the 4 GB droplet and plan.
    let tf = |cloud| cloud_module(cloud, &one_container(1024, 100), &host()).unwrap().contents;
    let size = |cloud| {
        let body = isoloom_test_support::hcl_body(&tf(cloud));
        isoloom_test_support::hcl_blocks(&body, "variable", &[])
            .iter()
            .filter(|v| {
                v.labels()
                    .first()
                    .is_some_and(|l| l.as_str().ends_with("size") || l.as_str() == "plan" || l.as_str() == "type")
            })
            .filter_map(|v| isoloom_test_support::hcl_attribute(v.body(), "default").map(ToString::to_string))
            .collect::<Vec<_>>()
    };
    assert!(
        size(Cloud::DigitalOcean).contains(&"\"s-4vcpu-8gb\"".to_owned()),
        "{:?}",
        size(Cloud::DigitalOcean)
    );
    assert!(size(Cloud::Linode).contains(&"\"g6-standard-4\"".to_owned()), "{:?}", size(Cloud::Linode));
}

#[test]
fn a_cloud_whose_largest_host_lacks_the_disk_is_left_out_and_the_others_kept() {
    let e = one_container(1024, 400);
    let refused: Vec<(Cloud, String)> = super::refusals(&e).into_iter().filter_map(|(c, why)| Some((c, why?))).collect();
    assert_eq!(refused.iter().map(|(c, _)| *c).collect::<Vec<_>>(), [Cloud::DigitalOcean, Cloud::Linode]);
    assert!(
        refused[0]
            .1
            .contains("the containers' host asks for a 400 GB disk: the largest DigitalOcean size here, `s-8vcpu-16gb`, has 320 GB"),
        "{}",
        refused[0].1
    );
    assert_eq!(refusal(&e), None, "the other clouds host them");
    let paths: Vec<String> = modules(&e).unwrap().into_iter().map(|f| f.path).collect();
    assert!(
        paths.iter().any(|p| p.contains("/aws/")) && !paths.iter().any(|p| p.contains("/digitalocean/") || p.contains("/linode/")),
        "{paths:?}"
    );
}

#[test]
fn no_cloud_hosting_the_containers_refuses_the_target() {
    let e = one_container(20480, 40);
    let why = refusal(&e).unwrap_or_default();
    assert!(
        why.starts_with("no cloud can run it (aws, azure, gcp, oci: the containers' host needs 21504 MB of memory"),
        "{why}"
    );
    assert!(why.contains("; digitalocean: the containers' host asks for 21 GB of memory"), "{why}");
}
