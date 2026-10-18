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
    Ok(testing::planned(&example(name)?, Target::CloudVm))
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
fn linode_hello_stack() -> Outcome {
    insta::assert_snapshot!(module(Cloud::Linode, "hello-stack")?);
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
        "DigitalOcean cloud-vm takes one machine on one network: a droplet joins one VPC, at a private address DigitalOcean picks, so other machines couldn't reach it at its spec address",
    );
    for name in ["hello-stack", "segmented"] {
        assert_eq!(
            refusals(name)?,
            [(Cloud::DigitalOcean, one_vm.clone()), (Cloud::Linode, None), (Cloud::Oci, None)],
            "{name}"
        );
    }
    assert_eq!(refusals("ansible-pair")?[1].1, s("Linode cloud-vm has no Ansible controller yet"));
    assert_eq!(refusals("windows-hello")?[2].1, s("Oracle Cloud cloud-vm is Linux only so far"));
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
    assert!(!has("digitalocean") && has("linode") && has("oci") && has("aws"));
    Ok(())
}

#[test]
fn an_os_without_an_image_is_refused() -> Outcome {
    let spec = testing::spec("version: 1\nname: r\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    os: rocky@9\n")?;
    let e = testing::planned(&spec, Target::CloudVm);
    let linode = CloudVmModule::new(Cloud::Linode).refusal(&e).expect("refused");
    assert_eq!(linode, "machine `w`: no Linode image for `rocky-9` yet (Debian 12/13, Ubuntu 22.04/24.04)");
    // Oracle Cloud runs Debian on Ubuntu, but no other OS: it would no longer be the spec's.
    let oci = CloudVmModule::new(Cloud::Oci).refusal(&e).expect("refused");
    assert_eq!(
        oci,
        "machine `w`: no Oracle Cloud image for `rocky-9` yet (Ubuntu 22.04/24.04, Debian 12/13 on Ubuntu 24.04)"
    );
    assert_eq!(CloudVmBackend.refusal(&e).map(|r| r.starts_with("no cloud can run it (")), Some(true));
    Ok(())
}
