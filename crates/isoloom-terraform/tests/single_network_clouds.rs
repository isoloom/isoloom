//! The DigitalOcean, Linode and Oracle Cloud modules of a few examples, rendered from their plan,
//! as snapshots; their determinism, and their refusals read from the plan without rendering.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::cloud_vm::{CLOUDS, CloudVmBackend, cloud_refusals};
use isoloom_terraform::cloud_vm_others::digitalocean::DigitalOceanBackend;
use isoloom_terraform::cloud_vm_others::linode::LinodeBackend;
use isoloom_terraform::cloud_vm_others::oci::OciBackend;

fn env(example: &str) -> Environment {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(example);
    let spec = isoloom_spec::load(&dir).expect("the example loads");
    assert!(isoloom_spec::validate(&spec).is_empty(), "the example is valid");
    Environment::build(&spec, Target::CloudVm, None)
}

/// The one module `backend` renders for `example`.
fn module(backend: &dyn Backend, example: &str) -> String {
    let files = backend.render(&env(example)).expect("the backend renders the example");
    assert_eq!(files.len(), 1, "one module");
    files.into_iter().next().expect("the module").contents
}

#[test]
fn digitalocean_solo_web() {
    insta::assert_snapshot!(module(&DigitalOceanBackend, "solo-web"));
}

#[test]
fn linode_solo_web() {
    insta::assert_snapshot!(module(&LinodeBackend, "solo-web"));
}

#[test]
fn oci_solo_web() {
    insta::assert_snapshot!(module(&OciBackend, "solo-web"));
}

#[test]
fn linode_hello_stack() {
    insta::assert_snapshot!(module(&LinodeBackend, "hello-stack"));
}

#[test]
fn oci_hello_stack() {
    insta::assert_snapshot!(module(&OciBackend, "hello-stack"));
}

#[test]
fn oci_opens_a_published_port_on_its_machine_only() {
    let m = module(&OciBackend, "hello-stack");
    let list = m.split("resource \"oci_core_security_list\"").nth(1).unwrap();
    let list = list.split("resource \"oci_core_subnet\"").next().unwrap();
    assert!(list.contains("SSH from allowed_cidr"), "the list read is the shared one");
    assert!(!list.contains("8080"), "the shared security list opens no published port");
    assert!(m.contains("resource \"oci_core_network_security_group\" \"web\""));
    assert!(!m.contains("resource \"oci_core_network_security_group\" \"cache\""), "cache publishes nothing");
    assert!(m.contains("= [oci_core_network_security_group.web.id]"));
    assert!(m.contains("resource \"oci_core_network_security_group_security_rule\" \"web_tcp_8080\""));
}

#[test]
fn rendering_twice_gives_the_same_files() {
    for example in ["solo-web", "hello-stack", "existing-hosts", "supplier-portal-api"] {
        let e = env(example);
        for (name, backend) in CLOUDS {
            assert_eq!(backend.render(&e), backend.render(&e), "{name} {example}");
        }
    }
}

/// Each single-network cloud's refusal of `example`, in `CLOUDS` order.
fn refusals(example: &str) -> Vec<(&'static str, Option<String>)> {
    cloud_refusals(&env(example))
        .into_iter()
        .filter(|(n, _)| ["digitalocean", "linode", "oci"].contains(n))
        .collect()
}

#[test]
fn refusals_keep_their_messages() {
    let s = |x: &str| Some(x.to_string());
    assert_eq!(
        refusals("hello-stack"),
        [
            (
                "digitalocean",
                s(
                    "DigitalOcean cloud-vm takes one machine on one network: a droplet joins one VPC, at a private address DigitalOcean picks, so other machines couldn't reach it at its spec address"
                )
            ),
            ("linode", None),
            ("oci", None),
        ]
    );
    assert_eq!(
        refusals("segmented"),
        [
            (
                "digitalocean",
                s(
                    "DigitalOcean cloud-vm takes one machine on one network: a droplet joins one VPC, at a private address DigitalOcean picks, so other machines couldn't reach it at its spec address"
                )
            ),
            ("linode", None),
            ("oci", None),
        ]
    );
    assert_eq!(refusals("ansible-pair")[1].1, s("Linode cloud-vm has no Ansible controller yet"));
    assert_eq!(refusals("windows-hello")[2].1, s("Oracle Cloud cloud-vm is Linux only so far"));
}

#[test]
fn a_refusal_matches_rendering_and_drops_only_that_cloud() {
    let e = env("hello-stack");
    let why = DigitalOceanBackend.refusal(&e).expect("refused");
    let err = DigitalOceanBackend.render(&e).expect_err("refused");
    assert!(err.to_string().ends_with(&why), "{err}");
    let files = CloudVmBackend.render(&e).expect("cloud-vm renders");
    let has = |sub: &str| files.iter().any(|f| f.path.ends_with(&format!("cloud-vm/{sub}/main.tf")));
    assert!(!has("digitalocean") && has("linode") && has("oci") && has("aws"));
}

#[test]
fn an_os_without_an_image_is_refused() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: r\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    vm: { os: rocky@9 }\n",
    )
    .unwrap();
    let e = Environment::build(&spec, Target::CloudVm, None);
    let why = LinodeBackend.refusal(&e).expect("refused");
    assert_eq!(why, "machine `w`: no Linode image for `rocky-9` yet (Debian 12/13, Ubuntu 22.04/24.04)");
    // Oracle Cloud runs Debian on Ubuntu, but no other OS: it would no longer be the spec's.
    let why = OciBackend.refusal(&e).expect("refused");
    assert_eq!(
        why,
        "machine `w`: no Oracle Cloud image for `rocky-9` yet (Ubuntu 22.04/24.04, Debian 12/13 on Ubuntu 24.04)"
    );
    assert_eq!(CloudVmBackend.refusal(&e).map(|r| r.starts_with("no cloud can run it (")), Some(true));
}
