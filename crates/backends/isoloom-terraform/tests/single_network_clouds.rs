//! The DigitalOcean, Linode and Oracle Cloud modules of a few examples, rendered from their plan,
//! as snapshots; their determinism, and their refusals read from the plan without rendering.

pub mod testing;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmBackend, CloudVmModule};
use testing::{Outcome, example};

/// Example `name`'s plan on `cloud-vm`.
fn env(name: &str) -> Outcome<Environment> {
    testing::planned(&example(name)?, Target::CloudVm)
}

/// `cloud`'s module for example `name`.
fn module(cloud: Cloud, name: &str) -> Outcome<String> {
    Ok(CloudVmModule::new(cloud).render(&env(name)?)?.contents)
}

#[test]
fn digitalocean_solo_web() -> Outcome {
    insta::assert_snapshot!(module(Cloud::DigitalOcean, "solo-web")?);
    Ok(())
}

#[test]
fn linode_solo_web() -> Outcome {
    insta::assert_snapshot!(module(Cloud::Linode, "solo-web")?);
    Ok(())
}

#[test]
fn oci_solo_web() -> Outcome {
    insta::assert_snapshot!(module(Cloud::Oci, "solo-web")?);
    Ok(())
}

#[test]
fn oci_hello_stack() -> Outcome {
    insta::assert_snapshot!(module(Cloud::Oci, "hello-stack")?);
    Ok(())
}

#[test]
fn oci_opens_a_published_port_on_its_machine_only() -> Outcome {
    let m = module(Cloud::Oci, "hello-stack")?;
    let list = m.split("resource \"oci_core_security_list\"").nth(1).unwrap();
    let list = list.split("resource \"oci_core_subnet\"").next().unwrap();
    assert!(list.contains("SSH from allowed_cidr"), "the list read is the shared one");
    assert!(!list.contains("8080"), "the shared security list opens no published port");
    assert!(m.contains("resource \"oci_core_network_security_group\" \"web\""));
    assert!(!m.contains("resource \"oci_core_network_security_group\" \"cache\""), "cache publishes nothing");
    assert!(m.contains("= [oci_core_network_security_group.web.id]"));
    assert!(m.contains("resource \"oci_core_network_security_group_security_rule\" \"web_tcp_8080\""));
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for name in ["solo-web", "hello-stack", "existing-hosts", "supplier-portal-api"] {
        let e = env(name)?;
        for module in Cloud::ALL.map(CloudVmModule::new) {
            assert_eq!(module.render(&e), module.render(&e), "{} {name}", module.cloud().id());
        }
    }
    Ok(())
}

/// Each single-network cloud's refusal of example `name`, in `Cloud::ALL`'s order.
fn refusals(name: &str) -> Outcome<Vec<(Cloud, Option<String>)>> {
    let single = [Cloud::DigitalOcean, Cloud::Linode, Cloud::Oci];
    Ok(CloudVmBackend.refusals(&env(name)?).into_iter().filter(|(c, _)| single.contains(c)).collect())
}

#[test]
fn refusals_keep_their_messages() -> Outcome {
    let s = |x: &str| Some(x.to_owned());
    let one_vm = s(
        "DigitalOcean cloud-vm takes one machine on one network: an instance joins one VPC, at a private address DigitalOcean picks, so other machines couldn't reach it at its spec address",
    );
    // Both have a network without internet, which Linode's firewalls couldn't close once the
    // machines are set up.
    for (name, net) in [("hello-stack", "app"), ("segmented", "back")] {
        let sealed = s(&format!(
            "network `{net}` has no internet: Linode can't seal an offline network (its firewalls only allow, and each machine's is made with it, so nothing made after the set-up can close its way out). Use `internet: true` (outbound only: nothing is reachable from outside but the published ports and SSH from allowed_cidr), or a provider that seals (AWS, Azure, GCP, OCI, Proxmox)"
        ));
        assert_eq!(
            refusals(name)?,
            [(Cloud::DigitalOcean, one_vm.clone()), (Cloud::Linode, sealed), (Cloud::Oci, None)],
            "{name}"
        );
    }
    assert_eq!(refusals("ansible-pair")?[1].1, s("Linode cloud-vm has no Ansible controller"));
    assert_eq!(refusals("windows-hello")?[2].1, s("Oracle Cloud cloud-vm is Linux only"));
    Ok(())
}

#[test]
fn a_refusal_matches_rendering_and_drops_only_that_cloud() -> Outcome {
    let e = env("hello-stack")?;
    let digitalocean = CloudVmModule::new(Cloud::DigitalOcean);
    let why = digitalocean.refusal(&e).expect("refused");
    let err = digitalocean.render(&e).expect_err("refused");
    assert!(err.to_string().ends_with(&why), "{err}");
    let files = CloudVmBackend.render(&e)?;
    let has = |sub: &str| files.iter().any(|f| f.path.ends_with(&format!("cloud-vm/{sub}/main.tf")));
    assert!(!has("digitalocean") && !has("linode") && has("oci") && has("aws"));
    Ok(())
}

#[test]
fn an_os_without_an_image_is_refused() -> Outcome {
    let spec = testing::spec("version: 1\nname: r\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    os: rocky@9\n")?;
    let e = testing::planned(&spec, Target::CloudVm)?;
    let linode = CloudVmModule::new(Cloud::Linode).refusal(&e).expect("refused");
    assert_eq!(linode, "machine `w`: no Linode image for `rocky-9` (Debian 12/13, Ubuntu 22.04/24.04)");
    // Oracle Cloud runs Debian on Ubuntu, but no other OS: it would no longer be the spec's.
    let oci = CloudVmModule::new(Cloud::Oci).refusal(&e).expect("refused");
    assert_eq!(
        oci,
        "machine `w`: no Oracle Cloud image for `rocky-9` (Ubuntu 22.04/24.04, Debian 12/13 on Ubuntu 24.04)"
    );
    assert_eq!(CloudVmBackend.refusal(&e).map(|r| r.starts_with("no cloud can run it (")), Some(true));
    Ok(())
}

/// With internet, DigitalOcean's and Linode's firewalls let nothing in from outside but SSH and
/// the published ports from allowed_cidr: every other inbound rule is from the environment's own
/// networks, and none from anywhere (`0.0.0.0/0`, `::/0`).
#[test]
fn an_open_network_on_the_allow_only_clouds_lets_in_ssh_and_the_published_ports_alone() -> Outcome {
    let yaml = "version: 1\nname: o\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12, ports: { site: { port: 80, publish: 8080 } }, app: { steps: [{ run: web.sh }] } }\n";
    for cloud in [Cloud::DigitalOcean, Cloud::Linode] {
        let tf = testing::cloud_vm(cloud, yaml)?;
        let mut from_outside = Vec::new();
        for block in inbound(&tf) {
            assert!(!block.contains("0.0.0.0/0") && !block.contains("::/0"), "{cloud:?}: {block}");
            if block.contains("var.allowed_cidr") {
                from_outside.push(block);
            }
        }
        assert!(!from_outside.is_empty(), "{cloud:?}: SSH from allowed_cidr");
        for block in from_outside {
            assert!(
                block.contains("\"22\"") || block.contains("\"8080\""),
                "{cloud:?}: only SSH and 8080 from outside: {block}"
            );
        }
        assert!(tf.contains("\"8080\""), "{cloud:?}: the published port");
    }
    Ok(())
}

/// The inbound rule blocks of a module: DigitalOcean's `inbound_rule`, Linode's `inbound`.
fn inbound(tf: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = tf.lines();
    while let Some(line) = lines.next() {
        let t = line.trim();
        if t == "inbound_rule {" || t == "inbound {" {
            let mut block = String::new();
            for l in lines.by_ref() {
                if l.trim() == "}" {
                    break;
                }
                block.push_str(l.trim());
                block.push(' ');
            }
            out.push(block);
        }
    }
    out
}
