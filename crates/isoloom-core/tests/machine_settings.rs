//! A machine's `environment`, `sysctls` and `ulimits`: from the container runtime on the
//! container targets, from the VM's own configuration (one set-up script) on the VM targets.

use std::path::Path;

use isoloom_core::{Spec, Target, generate, load, parse, refusal, validate};

fn tuned() -> Spec {
    load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/tuned-web")).unwrap()
}

fn file(spec: &Spec, t: Target, suffix: &str) -> String {
    generate(spec, t).unwrap().into_iter().find(|f| f.path.ends_with(suffix)).unwrap().contents
}

#[test]
fn compose_sets_them_on_the_container() {
    let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&file(&tuned(), Target::Docker, "compose.yml")).unwrap();
    let web = &doc["services"]["web"];
    assert_eq!(web["environment"]["APP_MODE"].as_str(), Some("production"));
    assert_eq!(web["sysctls"]["net.core.somaxconn"].as_str(), Some("4096"));
    assert_eq!(web["ulimits"]["nofile"]["soft"].as_u64(), Some(8192));
    assert_eq!(web["ulimits"]["nofile"]["hard"].as_u64(), Some(16384));
}

#[test]
fn every_vm_target_runs_the_same_settings_before_the_steps() {
    let spec = tuned();
    let vf = file(&spec, Target::Vagrant, "vagrant/Vagrantfile");
    let settings = vf.find("name: \"settings\"").expect("the settings step");
    assert!(settings < vf.find("name: \"provision/web.sh\"").unwrap());
    for needle in [
        "APP_MODE=production",
        "net.core.somaxconn = 4096",
        "soft nofile 8192",
        "DefaultLimitNOFILE=8192:16384",
        "daemon-reexec",
    ] {
        assert!(vf.contains(needle), "Vagrant: {needle}");
    }
    let pve = file(&spec, Target::Proxmox, "proxmox/main.tf");
    assert!(pve.contains("net.core.somaxconn = 4096"), "Proxmox runcmd");
    for cloud in ["aws", "azure", "gcp", "linode", "oci"] {
        let tf = file(&spec, Target::CloudVm, &format!("cloud-vm/{cloud}/main.tf"));
        assert!(tf.contains("sudo sh -c") && tf.contains("APP_MODE=production"), "{cloud}");
    }
}

#[test]
fn kubernetes_refuses_ulimits_and_sets_the_rest() {
    assert!(refusal(&tuned(), Target::Kubernetes).unwrap().contains("process limits"));
    let mut spec = tuned();
    spec.machines["web"].ulimits.clear();
    let all: String = generate(&spec, Target::Kubernetes).unwrap().into_iter().map(|f| f.contents).collect();
    assert!(all.contains("name: APP_MODE") && all.contains("value: production"));
    assert!(all.contains("name: net.core.somaxconn"));
}

#[test]
fn the_fields_are_validated() {
    let spec = parse(
        "version: 1
name: bad
networks: { app: { cidr: 10.83.0.0/24 } }
machines:
  web:
    networks: { app: 10 }
    environment: { 1BAD: x }
    sysctls: { vm.swappiness: 10, Bad: 1 }
    ulimits: { nofiles: 1, nproc: { soft: 9, hard: 1 } }
    docker: { image: nginx }
  dc:
    networks: { app: 11 }
    sysctls: { net.core.somaxconn: 1 }
    vm: { os: windows-server-2019, provision: [dc.ps1] }
",
    )
    .unwrap();
    let problems: Vec<String> = validate(&spec).into_iter().map(|p| p.to_string()).collect();
    let has = |at: &str, what: &str| problems.iter().any(|p| p.starts_with(at) && p.contains(what));
    assert!(has("machines.web.environment.1BAD", "variable name"), "{problems:#?}");
    assert!(has("machines.web.sysctls.vm.swappiness", "namespaced"), "{problems:#?}");
    assert!(has("machines.web.sysctls.Bad", "kernel parameter"), "{problems:#?}");
    assert!(has("machines.web.ulimits.nofiles", "one of"), "{problems:#?}");
    assert!(has("machines.web.ulimits.nproc", "soft limit"), "{problems:#?}");
    assert!(has("machines.dc.sysctls", "Linux only"), "{problems:#?}");
    let e = parse("version: 1\nname: x\nnetworks: {}\nmachines: { a: { environment: { K: [1] } } }\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("a text, a number or a boolean"), "{e}");
}
