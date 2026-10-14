//! The docker-vm and hybrid backends' rendering of examples, as snapshots; their determinism;
//! and their refusals, which come from the plan without rendering.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::files::GeneratedFile;
use isoloom_spec::model::Target;
use isoloom_vagrant::{DockerVmBackend, HybridBackend};

fn plan(example: &str, target: Target) -> Environment {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(example);
    let spec = isoloom_spec::load(&dir).expect("the example loads");
    assert!(isoloom_spec::validate(&spec).is_empty(), "the example is valid");
    Environment::build(&spec, target, None)
}

fn render(backend: &dyn Backend, example: &str) -> Vec<GeneratedFile> {
    backend.render(&plan(example, backend.target())).expect("the example renders")
}

fn file(files: &[GeneratedFile], path: &str) -> String {
    files
        .iter()
        .find(|f| f.path.ends_with(path))
        .unwrap_or_else(|| panic!("{path}"))
        .contents
        .clone()
}

#[test]
fn docker_vm_hello_stack() {
    let files = render(&DockerVmBackend, "hello-stack");
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&".isoloom/docker/compose.yml"), "the Compose file it runs: {paths:?}");
    assert!(paths.contains(&".isoloom/docker-vm/proxmox/main.tf"), "{paths:?}");
    insta::assert_snapshot!(file(&files, "docker-vm/Vagrantfile"));
}

#[test]
fn docker_vm_segmented() {
    insta::assert_snapshot!(file(&render(&DockerVmBackend, "segmented"), "docker-vm/Vagrantfile"));
}

#[test]
fn hybrid_mixed_office_vagrantfile() {
    insta::assert_snapshot!(file(&render(&HybridBackend, "mixed-office"), "hybrid/Vagrantfile"));
}

#[test]
fn hybrid_mixed_office_compose() {
    insta::assert_snapshot!(file(&render(&HybridBackend, "mixed-office"), "hybrid/compose.yml"));
}

#[test]
fn hybrid_files_are_the_vagrantfile_the_compose_file_then_the_checks() {
    let files = render(&HybridBackend, "mixed-office");
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            ".isoloom/hybrid/Vagrantfile",
            ".isoloom/hybrid/compose.yml",
            ".isoloom/hybrid/checks/controller.sh"
        ]
    );
}

#[test]
fn rendering_twice_gives_the_same_files() {
    for example in ["hello-stack", "segmented", "udp-dns"] {
        assert_eq!(render(&DockerVmBackend, example), render(&DockerVmBackend, example), "{example}");
    }
    assert_eq!(render(&HybridBackend, "mixed-office"), render(&HybridBackend, "mixed-office"));
}

/// A container `web` and a VM `db` on one network, with `extra` appended.
fn mixed(extra: &str) -> Environment {
    let text = format!(
        "version: 1\nname: m\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  web:\n    networks: {{ lab: 10 }}\n    ports: [{{ port: 80 }}]\n    docker: {{ image: nginx }}\n  db:\n    networks: {{ lab: 20 }}\n    ports: [{{ port: 5432 }}]\n    vm: {{ os: debian-12 }}\n{extra}"
    );
    let spec = isoloom_spec::parse(&text).expect("parses");
    Environment::build(&spec, Target::Hybrid, None)
}

#[test]
fn hybrid_refuses_a_vm_waiting_for_a_container_from_the_plan() {
    let env = mixed("    depends_on: [web]\n");
    let why = HybridBackend.refusal(&env).expect("refused");
    assert_eq!(why, "VM `db` depends on container `web`: VMs start first in a hybrid environment, for now");
    assert_eq!(
        HybridBackend.render(&env).unwrap_err().to_string(),
        format!("hybrid: {why}"),
        "render refuses the same way, before writing anything"
    );
}

#[test]
fn hybrid_refuses_a_volume_shared_by_containers_and_vms() {
    let mut env = mixed("volumes:\n  data: { owner: db }\n");
    let spec = {
        let mut s = env.spec().clone();
        s.machines["web"].volumes.insert("data".into(), "/srv".into());
        s.machines["db"].volumes.insert("data".into(), "/var/data".into());
        s
    };
    env = Environment::build(&spec, Target::Hybrid, None);
    assert!(HybridBackend.refusal(&env).expect("refused").contains("`data` is shared by containers and VMs"));
}

#[test]
fn hybrid_refuses_gateways_and_docker_vm_takes_them() {
    let gateway = "version: 1\nname: g\nnetworks:\n  lan: { cidr: 10.9.0.0/24, gateway: fw }\n  wan: { cidr: 10.8.0.0/24 }\nmachines:\n  fw:\n    networks: { wan: 2, lan: 1 }\n    docker: { image: alpine }\n    vm: { os: debian-12 }\n  pc:\n    networks: { lan: 10 }\n    vm: { os: debian-12 }\n";
    let spec = isoloom_spec::parse(gateway).expect("parses");
    let why = HybridBackend.refusal(&Environment::build(&spec, Target::Hybrid, None));
    assert_eq!(why.as_deref(), Some("networks with a `gateway` machine in hybrid environments come later"));
    assert!(DockerVmBackend.refusal(&Environment::build(&spec, Target::DockerVm, None)).is_none());
}

#[test]
fn docker_vm_refuses_what_compose_cannot_run() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: p\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    networks: { lab: 10 }\n    docker: { image: alpine }\nprovision:\n  - ansible: site.yml\n",
    )
    .expect("parses");
    let env = Environment::build(&spec, Target::DockerVm, None);
    assert_eq!(
        DockerVmBackend.render(&env).unwrap_err().to_string(),
        "docker-vm: environment-level provisioning (`provision:`) runs on VM targets for now"
    );
}

#[test]
fn hybrid_forwards_a_fixed_published_port() {
    let mut spec = mixed("").spec().clone();
    let web = &mut spec.machines["web"].ports[0];
    web.publish = Some(8080);
    web.fixed = true;
    let files = HybridBackend.render(&Environment::build(&spec, Target::Hybrid, None)).expect("renders");
    assert!(file(&files, "hybrid/compose.yml").contains("    - 8080:80\n"));
    let forward = "m.vm.network \"forwarded_port\", guest: 8080, host: 8080, host_ip: \"127.0.0.1\"\n";
    assert!(file(&files, "hybrid/Vagrantfile").contains(forward));
}

#[test]
fn only_containers_and_vms_make_an_environment_mixed() {
    // An external machine waiting for a container is neither a VM nor a container: the Docker
    // targets still render the environment.
    let text = "version: 1\nname: x\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  cache:\n    networks: { lab: 20 }\n    ports: [{ port: 6379 }]\n    docker: { image: redis }\n  web:\n    networks: { lab: 10 }\n    depends_on: [cache]\n    external: { address: 10.9.0.10 }\n";
    let spec = isoloom_spec::parse(text).expect("parses");
    for target in [Target::DockerVm, Target::Hybrid] {
        assert!(Environment::build(&spec, target, None).requirements.is_empty(), "{target:?}");
    }
}

#[test]
fn hybrid_forwards_a_published_tool() {
    let mut spec = mixed("").spec().clone();
    let tools = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    networks: { lab: 10 }\n    docker: { image: alpine }\ntools:\n  web-ui: { image: \"nginx:1\", port: 80, publish: 9090 }\n";
    spec.tools = isoloom_spec::parse(tools).expect("parses").tools;
    let files = HybridBackend.render(&Environment::build(&spec, Target::Hybrid, None)).expect("renders");
    let compose = file(&files, "hybrid/compose.yml");
    assert!(compose.contains("    - 9090:80\n"), "{compose}");
    assert!(compose.contains("      isoloom-publish: null\n"), "{compose}");
    let forward = "m.vm.network \"forwarded_port\", guest: 9090, host: 9090, host_ip: \"127.0.0.1\"\n";
    assert!(file(&files, "hybrid/Vagrantfile").contains(forward));
}
