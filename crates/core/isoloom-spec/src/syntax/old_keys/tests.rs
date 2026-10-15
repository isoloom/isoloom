use super::*;

fn scanned(yaml: &str) -> Vec<Problem> {
    scan(&serde_yaml_ng::from_str(yaml).unwrap())
}

#[test]
fn a_docker_block_gets_its_app() {
    let p = scanned("machines:\n  web:\n    docker: { build: build/web, init: [flag.sh] }\n");
    assert_eq!((p[0].code, p[0].path.as_str()), (Code::RemovedField, "machines.web.docker"));
    let fix = p[0].fix.as_deref().unwrap();
    assert!(
        fix.contains("`app: { build: build/web }`") && fix.contains("setup") && fix.contains("input"),
        "{fix}"
    );
    let p = scanned("machines:\n  bench:\n    docker: { image: alpine:3.20, idle: true }\n");
    assert!(p[0].fix.as_deref().unwrap().contains("`os: <family>@<version>` alone"));
}

#[test]
fn a_vm_block_gets_its_os_and_steps() {
    let p = scanned("machines:\n  cache:\n    vm: { os: debian@12, provision: [a.sh, reboot, b.yml], tpm: true }\n");
    let fix = p[0].fix.as_deref().unwrap();
    assert!(
        fix.contains("`os: debian@12`") && fix.contains("[{ run: a.sh }, reboot, { playbook: b.yml }]") && fix.contains("tpm: required"),
        "{fix}"
    );
}

#[test]
fn a_list_of_ports_becomes_a_map() {
    let p = scanned("machines:\n  web:\n    ports: [{ port: 80, http: true, publish: 8080 }, { port: 3206, name: portal, publish: 3206, fixed: true }]\n");
    assert_eq!(p[0].code, Code::InvalidValue);
    let fix = p[0].fix.as_deref().unwrap();
    assert!(fix.contains("p80: { port: 80, protocol: http, publish: 8080 }"), "{fix}");
    assert!(fix.contains("portal: { port: 3206, publish: { host: 3206, fixed: true } }"), "{fix}");
}

#[test]
fn moved_keys_name_their_new_place() {
    let p = scanned(
        "networks:\n  lab: { cidr: 192.168.1.0/24, docker: { cidr: 10.1.0.0/24 } }\nvolumes:\n  data: { external: shared }\ncommon:\n  vm: { os: debian }\n  inputs: [FLAG]\ncontroller: { image: { vagrant: x } }\nmachines:\n  web: { arch: arm64, inputs: [FLAG], external: { address: 1.2.3.4 } }\n",
    );
    let paths: Vec<&str> = p.iter().map(|p| p.path.as_str()).collect();
    for path in [
        "machines.web.external",
        "machines.web.inputs",
        "machines.web.arch",
        "common.inputs",
        "common.vm",
        "networks.lab.docker",
        "volumes.data.external",
        "controller.image",
    ] {
        assert!(paths.contains(&path), "{path} in {paths:?}");
    }
    assert!(p.iter().all(|p| p.fix.is_some()));
}

#[test]
fn a_spec_in_the_new_form_has_none() {
    assert_eq!(scanned("machines:\n  web: { os: debian@12, app: { image: nginx }, ports: { web: 80 } }\n"), []);
}
