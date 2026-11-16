//! The `cloud-vm` modules of a few examples on every cloud, rendered from their plan, as
//! snapshots; their determinism; and each cloud's refusals, read from the plan without rendering
//! and dropping only that cloud's module.

use crate::testing::{Outcome, example, file, planned, spec};
use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmBackend, CloudVmModule};
use isoloom_test_support::{assert_every, hcl_attribute, hcl_body, hcl_literals, hcl_names, hcl_nested, hcl_one, hcl_resource, hcl_text, hcl_values};

/// Example `name`'s plan on `cloud-vm`.
fn env(name: &str) -> Outcome<Environment> {
    planned(&example(name)?, Target::CloudVm)
}

/// `cloud`'s module for example `name`.
fn module(cloud: Cloud, name: &str) -> Outcome<String> {
    Ok(CloudVmModule::new(cloud).render(&env(name)?)?.contents)
}

/// The modules a reviewer reads as files: each cloud's module of the examples it takes.
#[test]
fn each_clouds_module_of_the_examples() -> Outcome {
    for (cloud, name) in [
        (Cloud::Aws, "hello-stack"),
        (Cloud::Aws, "segmented"),
        (Cloud::Aws, "windows-hello"),
        (Cloud::Aws, "ansible-pair"),
        (Cloud::Azure, "hello-stack"),
        (Cloud::Azure, "segmented"),
        (Cloud::Azure, "windows-hello"),
        (Cloud::Gcp, "solo-web"),
        (Cloud::Gcp, "hello-stack"),
        (Cloud::DigitalOcean, "solo-web"),
        (Cloud::Linode, "solo-web"),
        (Cloud::Oci, "solo-web"),
        (Cloud::Oci, "hello-stack"),
    ] {
        insta::assert_snapshot!(format!("{}_{}", cloud.id(), name.replace('-', "_")), module(cloud, name)?);
    }
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for name in [
        "solo-web",
        "hello-stack",
        "segmented",
        "vlan-office",
        "existing-hosts",
        "windows-hello",
        "supplier-portal-api",
        "invoice-portal-api",
    ] {
        let e = env(name)?;
        for module in Cloud::ALL.map(CloudVmModule::new) {
            assert_eq!(module.render(&e), module.render(&e), "{} {name}", module.cloud().id());
        }
    }
    Ok(())
}

/// One arm64 machine: Proxmox runs it (an emulated aarch64 VM); every cloud refuses it.
const ARM64: &str = "version: 1\nname: r\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    hardware: { arch: arm64 }\n    os: debian@12\n";

#[test]
fn refusals_come_from_the_plan_and_match_rendering() -> Outcome {
    let why = "no cloud can run it (aws: machine `w`: Isoloom generates no arm64 VM on AWS; \
         azure: machine `w`: Isoloom generates no arm64 VM on Azure; \
         gcp: machine `w`: Isoloom generates no arm64 VM on Google Cloud; \
         digitalocean: machine `w`: DigitalOcean has no arm64 droplets; \
         linode: machine `w`: Linode has no arm64 plans; \
         oci: machine `w`: Isoloom generates no arm64 VM on Oracle Cloud)";
    let e = planned(&spec(ARM64)?, CloudVmBackend.target())?;
    assert_eq!(CloudVmBackend.refusal(&e).as_deref(), Some(why));
    let err = CloudVmBackend.render(&e).expect_err("refused");
    assert!(err.to_string().ends_with(why), "{err}");
    // Each cloud's own module says its part, rendering or not.
    for module in Cloud::ALL.map(CloudVmModule::new) {
        let own = module.refusal(&e).ok_or(format!("{} takes arm64", module.cloud()))?;
        assert!(why.contains(&format!("{}: {own}", module.cloud().id())), "{own}");
        let rendered = module.render(&e).expect_err("refused");
        assert!(rendered.to_string().ends_with(&own), "{rendered}");
    }
    Ok(())
}

#[test]
fn each_cloud_refuses_on_its_own() -> Outcome {
    // Two networks, each machine on one: AWS, Azure and Google Cloud take it.
    let refusals = CloudVmBackend.refusals(&env("segmented")?);
    let clouds: Vec<Cloud> = refusals.iter().map(|(c, _)| *c).collect();
    assert_eq!(clouds, Cloud::ALL);
    assert_every!(&refusals[..3], |(_, why): &&(Cloud, Option<String>)| why.is_none());
    // A Windows machine: Google Cloud only.
    let windows = env("windows-hello")?;
    let on_windows = CloudVmBackend.refusals(&windows);
    assert_eq!(
        on_windows[2].1.as_deref(),
        Some("machine `web01`: Isoloom generates no Windows VM on Google Cloud")
    );
    let files = CloudVmBackend.render(&windows)?;
    assert!(files.iter().any(|f| f.path.ends_with("cloud-vm/azure/main.tf")));
    assert!(!files.iter().any(|f| f.path.ends_with("cloud-vm/gcp/main.tf")));
    Ok(())
}

/// Each single-network cloud's refusal of example `name`, in `Cloud::ALL`'s order.
fn single_network_refusals(name: &str) -> Outcome<Vec<(Cloud, Option<String>)>> {
    let single = [Cloud::DigitalOcean, Cloud::Linode, Cloud::Oci];
    Ok(CloudVmBackend.refusals(&env(name)?).into_iter().filter(|(c, _)| single.contains(c)).collect())
}

#[test]
fn the_single_network_clouds_refuse_with_their_reasons() -> Outcome {
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
            single_network_refusals(name)?,
            [(Cloud::DigitalOcean, one_vm.clone()), (Cloud::Linode, sealed), (Cloud::Oci, None)],
            "{name}"
        );
    }
    assert_eq!(single_network_refusals("ansible-pair")?[1].1, s("Linode cloud-vm has no Ansible controller"));
    assert_eq!(single_network_refusals("windows-hello")?[2].1, s("Oracle Cloud cloud-vm is Linux only"));
    Ok(())
}

#[test]
fn a_refusal_matches_rendering_and_drops_only_that_cloud() -> Outcome {
    let e = env("hello-stack")?;
    let digitalocean = CloudVmModule::new(Cloud::DigitalOcean);
    let why = digitalocean.refusal(&e).ok_or("DigitalOcean takes hello-stack")?;
    let err = digitalocean.render(&e).expect_err("refused");
    assert!(err.to_string().ends_with(&why), "{err}");
    let files = CloudVmBackend.render(&e)?;
    let has = |sub: &str| files.iter().any(|f| f.path.ends_with(&format!("cloud-vm/{sub}/main.tf")));
    assert!(!has("digitalocean") && !has("linode") && has("oci") && has("aws"));
    Ok(())
}

#[test]
fn an_os_without_an_image_is_refused() -> Outcome {
    let spec = spec("version: 1\nname: r\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    os: rocky@9\n")?;
    let e = planned(&spec, Target::CloudVm)?;
    let linode = CloudVmModule::new(Cloud::Linode).refusal(&e);
    assert_eq!(
        linode.as_deref(),
        Some("machine `w`: no Linode image for `rocky-9` (Debian 12/13, Ubuntu 22.04/24.04)")
    );
    // Oracle Cloud runs Debian on Ubuntu, but no other OS: it would no longer be the spec's.
    let oci = CloudVmModule::new(Cloud::Oci).refusal(&e);
    assert_eq!(
        oci.as_deref(),
        Some("machine `w`: no Oracle Cloud image for `rocky-9` (Ubuntu 22.04/24.04, Debian 12/13 on Ubuntu 24.04)")
    );
    assert_eq!(CloudVmBackend.refusal(&e).map(|r| r.starts_with("no cloud can run it (")), Some(true));
    Ok(())
}

/// A spec of one machine, `w`, with `fields` (its OS, steps and hardware).
fn one_vm(fields: &str) -> Outcome<isoloom_plan::Lab> {
    spec(&format!(
        "version: 1\nname: t\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  w: {{ networks: {{ lab: 10 }}, {fields} }}\n"
    ))
}

#[test]
fn a_tpm_is_given_on_the_clouds_that_have_one_and_refused_where_none_is() -> Outcome {
    let tpm = one_vm("os: debian@12, app: { steps: [{ run: a.sh }] }, hardware: { tpm: required }")?;
    let cloud = planned(&tpm, Target::CloudVm)?;
    let clouds = CloudVmBackend.render(&cloud)?;
    let vm = |id: &str, kind: &str| -> Outcome<hcl::Body> { Ok(hcl_resource(file(&clouds, &format!("cloud-vm/{id}/main.tf"))?, kind, "w")) };
    // AWS: NitroTPM comes with the AMI, which the plan checks before starting the VM.
    let aws = vm("aws", "aws_instance")?;
    let preconditions: Vec<String> = hcl_nested(&aws, "lifecycle")
        .iter()
        .flat_map(|l| hcl_nested(l.body(), "precondition"))
        .map(|p| hcl_text(p.body(), "condition"))
        .collect();
    assert!(preconditions.iter().any(|c| c.ends_with(".tpm_support == \"v2.0\"")), "{preconditions:?}");
    for (id, kind, key) in [
        ("azure", "azurerm_linux_virtual_machine", "vtpm_enabled"),
        ("gcp", "google_compute_instance", "enable_vtpm"),
        ("oci", "oci_core_instance", "is_trusted_platform_module_enabled"),
    ] {
        assert_eq!(hcl_values(&vm(id, kind)?, key), ["true"], "{id}");
    }
    let digitalocean = CloudVmModule::new(Cloud::DigitalOcean).refusal(&cloud).ok_or("DigitalOcean gives a TPM")?;
    assert!(
        digitalocean.contains("asks for a virtual TPM (`hardware.tpm: required`): DigitalOcean gives none"),
        "{digitalocean}"
    );
    // Windows with a TPM: AWS refuses it, which drops only its module; Azure's trusted launch is
    // written.
    let windows = planned(
        &one_vm("os: windows-server@2022, app: { steps: [{ run: a.ps1 }] }, hardware: { tpm: required }")?,
        Target::CloudVm,
    )?;
    let on_aws = CloudVmModule::new(Cloud::Aws).refusal(&windows).ok_or("AWS gives Windows a TPM")?;
    assert!(on_aws.contains("NitroTPM"), "{on_aws}");
    assert_eq!(CloudVmBackend.refusal(&windows), None);
    let rendered = CloudVmBackend.render(&windows)?;
    assert!(!rendered.iter().any(|f| f.path.contains("cloud-vm/aws/")));
    let azure = hcl_resource(file(&rendered, "cloud-vm/azure/main.tf")?, "azurerm_windows_virtual_machine", "w");
    assert_eq!(hcl_values(&azure, "vtpm_enabled"), ["true"]);
    // Windows 11 merely expects one: the clouds leave it out instead of refusing.
    let wanted = planned(&one_vm("os: windows@11, app: { steps: [{ run: a.ps1 }] }")?, Target::CloudVm)?;
    assert_eq!(wanted.machines["w"].tpm, Some(isoloom_spec::windows::Tpm::Wanted));
    assert!(isoloom_plan::refusal(&wanted.requirements, &CloudVmBackend.capabilities()).is_none());
    Ok(())
}

/// Attribute `key` of `body` as HCL writes it, without its whitespace (`[var.allowed_cidr]`).
fn expression(body: &hcl::Body, key: &str) -> Option<String> {
    let expr = hcl_attribute(body, key)?;
    Some(hcl::format::to_string(expr).ok()?.split_whitespace().collect())
}

#[test]
fn oracle_cloud_opens_a_published_port_in_its_machines_own_security_group() -> Outcome {
    let tf = hcl_body(&module(Cloud::Oci, "hello-stack")?);
    // web publishes 8080; cache, beside it, nothing: one group, web's, attached to web alone.
    assert_eq!(hcl_names(&tf, "resource", "oci_core_network_security_group"), ["web"]);
    assert_eq!(hcl_names(&tf, "resource", "oci_core_network_security_group_security_rule"), ["web_tcp_8080"]);
    let nsgs = |name: &str| {
        hcl_nested(hcl_one(&tf, "oci_core_instance", name), "create_vnic_details")
            .iter()
            .filter_map(|v| expression(v.body(), "nsg_ids"))
            .collect::<Vec<_>>()
    };
    assert_eq!(nsgs("web"), ["[oci_core_network_security_group.web.id]"]);
    assert_eq!(nsgs("cache"), Vec::<String>::new());
    // The shared security list opens SSH, never the published port.
    let shared = hcl_one(&tf, "oci_core_security_list", "app");
    assert!(!hcl_literals(shared).iter().any(|l| l == "8080"), "the shared list opens 8080");
    Ok(())
}

/// With internet, DigitalOcean's and Linode's firewalls let nothing in from outside but SSH and
/// the published ports from `allowed_cidr`: every other inbound rule is from the environment's own
/// networks, and none from anywhere (`0.0.0.0/0`, `::/0`).
#[test]
fn an_open_network_on_the_allow_only_clouds_lets_in_ssh_and_the_published_ports_alone() -> Outcome {
    let yaml = "version: 1\nname: o\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12, ports: { site: { port: 80, publish: 8080 } }, app: { steps: [{ run: web.sh }] } }\n";
    for (cloud, firewall, rule, sources, ports) in [
        (Cloud::DigitalOcean, "digitalocean_firewall", "inbound_rule", "source_addresses", "port_range"),
        (Cloud::Linode, "linode_firewall", "inbound", "ipv4", "ports"),
    ] {
        let tf = hcl_resource(&crate::testing::cloud_vm(cloud, yaml)?, firewall, "web");
        let rules: Vec<(String, String)> = hcl_nested(&tf, rule)
            .iter()
            .map(|r| (expression(r.body(), sources).unwrap_or_default(), hcl_values(r.body(), ports).join("")))
            .collect();
        assert_every!(&rules, |(from, _): &&(String, String)| !from.contains("0.0.0.0/0") && !from.contains("::/0"));
        let mut from_outside: Vec<&str> = rules
            .iter()
            .filter(|(from, _)| from == "[var.allowed_cidr]")
            .map(|(_, port)| port.as_str())
            .collect();
        from_outside.sort_unstable();
        assert_eq!(from_outside, ["22", "8080"], "{cloud}: only SSH and the published port from outside");
    }
    Ok(())
}
