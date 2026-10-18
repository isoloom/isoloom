//! The docker-vm and hybrid backends' rendering of examples, as snapshots; their determinism;
//! and their refusals, which come from the plan without rendering.

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::files::GeneratedFile;
use isoloom_spec::model::Target;
use isoloom_targets::{DockerVmBackend, HybridBackend, Lab};

/// A test's outcome: a failure is the report.
type Outcome<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// The repository's example `name`, valid, rendered by `backend`.
fn render(backend: &dyn Backend, name: &str) -> Outcome<Vec<GeneratedFile>> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name);
    let lab = Lab::new(isoloom_spec::load(&dir)?);
    if !lab.problems().is_empty() {
        return Err(format!("{name} isn't valid: {:?}", lab.problems()).into());
    }
    let target = backend.target();
    Ok(backend.render(&Environment::build(&lab.for_target(target), target, None))?)
}

/// `yaml`, planned for `target` (whether or not validation would accept it).
fn build(yaml: &str, target: Target) -> Outcome<Environment> {
    let lab = Lab::parse(yaml).map_err(|problems| format!("{problems:?}"))?;
    Ok(Environment::build(&lab.for_target(target), target, None))
}

/// The text of the file among `files` whose path ends with `path`.
fn file(files: &[GeneratedFile], path: &str) -> Outcome<String> {
    Ok(files
        .iter()
        .find(|f| f.path.ends_with(path))
        .ok_or_else(|| format!("no {path}"))?
        .contents
        .clone())
}

#[test]
fn docker_vm_hello_stack() -> Outcome {
    let files = render(&DockerVmBackend, "hello-stack")?;
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&".isoloom/docker/compose.yml"), "the Compose file it runs: {paths:?}");
    assert!(paths.contains(&".isoloom/docker-vm/proxmox/main.tf"), "{paths:?}");
    insta::assert_snapshot!(file(&files, "docker-vm/Vagrantfile")?);
    Ok(())
}

#[test]
fn docker_vm_segmented() -> Outcome {
    insta::assert_snapshot!(file(&render(&DockerVmBackend, "segmented")?, "docker-vm/Vagrantfile")?);
    Ok(())
}

#[test]
fn hybrid_mixed_office_vagrantfile() -> Outcome {
    insta::assert_snapshot!(file(&render(&HybridBackend, "mixed-office")?, "hybrid/Vagrantfile")?);
    Ok(())
}

#[test]
fn hybrid_mixed_office_compose() -> Outcome {
    insta::assert_snapshot!(file(&render(&HybridBackend, "mixed-office")?, "hybrid/compose.yml")?);
    Ok(())
}

#[test]
fn hybrid_files_are_the_vagrantfile_the_compose_file_then_the_checks() -> Outcome {
    let files = render(&HybridBackend, "mixed-office")?;
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            ".isoloom/hybrid/Vagrantfile",
            ".isoloom/hybrid/compose.yml",
            ".isoloom/hybrid/checks/controller.sh"
        ]
    );
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for example in ["hello-stack", "segmented", "udp-dns"] {
        assert_eq!(render(&DockerVmBackend, example)?, render(&DockerVmBackend, example)?, "{example}");
    }
    assert_eq!(render(&HybridBackend, "mixed-office")?, render(&HybridBackend, "mixed-office")?);
    Ok(())
}

/// A container `web` and a VM `db` (a TPM rules out its container form) on one network, with
/// `extra` appended.
fn mixed(extra: &str) -> Outcome<Environment> {
    let text = format!(
        "version: 1\nname: m\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  web:\n    networks: {{ lab: 10 }}\n    ports: {{ web: 80 }}\n    app: {{ image: nginx }}\n  db:\n    os: debian@12\n    networks: {{ lab: 20 }}\n    ports: {{ db: 5432 }}\n    hardware: {{ tpm: required }}\n{extra}"
    );
    build(&text, Target::Hybrid)
}

#[test]
fn hybrid_refuses_a_vm_waiting_for_a_container_from_the_plan() -> Outcome {
    let env = mixed("    depends_on: [web]\n")?;
    let why = HybridBackend.refusal(&env).expect("refused");
    assert_eq!(why, "VM `db` depends on container `web`: VMs start first in a hybrid environment, for now");
    assert_eq!(
        HybridBackend.render(&env).unwrap_err().to_string(),
        format!("hybrid: {why}"),
        "render refuses the same way, before writing anything"
    );
    Ok(())
}

#[test]
fn hybrid_refuses_a_volume_shared_by_containers_and_vms() -> Outcome {
    let mut env = mixed("volumes:\n  data: { owner: db }\n")?;
    let spec = {
        let mut s = env.spec().clone();
        s.machines["web"].volumes.insert("data".into(), "/srv".into());
        s.machines["db"].volumes.insert("data".into(), "/var/data".into());
        s
    };
    env = Environment::build(&spec, Target::Hybrid, None);
    assert!(HybridBackend.refusal(&env).expect("refused").contains("`data` is shared by containers and VMs"));
    Ok(())
}

#[test]
fn hybrid_refuses_gateways_and_docker_vm_takes_them() -> Outcome {
    // `fw` has both forms (Debian's image, a Debian VM); a TPM makes `pc` a VM.
    let gateway = "version: 1\nname: g\nnetworks:\n  lan: { cidr: 10.9.0.0/24, gateway: fw }\n  wan: { cidr: 10.8.0.0/24 }\nmachines:\n  fw:\n    os: debian@12\n    networks: { wan: 2, lan: 1 }\n  pc:\n    os: debian@12\n    networks: { lan: 10 }\n    hardware: { tpm: required }\n";
    let why = HybridBackend.refusal(&build(gateway, Target::Hybrid)?);
    assert_eq!(why.as_deref(), Some("networks with a `gateway` machine in hybrid environments come later"));
    assert!(DockerVmBackend.refusal(&build(gateway, Target::DockerVm)?).is_none());
    Ok(())
}

#[test]
fn docker_vm_refuses_what_compose_cannot_run() -> Outcome {
    let env = build(
        "version: 1\nname: p\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    networks: { lab: 10 }\n    app: { image: alpine }\nprovision:\n  - ansible: site.yml\n",
        Target::DockerVm,
    )?;
    assert_eq!(
        DockerVmBackend.render(&env).unwrap_err().to_string(),
        "docker-vm: environment-level provisioning (`provision:`) runs on VM targets for now"
    );
    Ok(())
}

#[test]
fn hybrid_forwards_a_fixed_published_port() -> Outcome {
    let mut spec = mixed("")?.spec().clone();
    let web = &mut spec.machines["web"].ports[0];
    web.publish = Some(8080);
    web.fixed = true;
    let files = HybridBackend.render(&Environment::build(&spec, Target::Hybrid, None)).expect("renders");
    assert!(file(&files, "hybrid/compose.yml")?.contains("    - 8080:80\n"));
    let forward = "m.vm.network \"forwarded_port\", guest: 8080, host: 8080, host_ip: \"127.0.0.1\"\n";
    assert!(file(&files, "hybrid/Vagrantfile")?.contains(forward));
    Ok(())
}

#[test]
fn only_containers_and_vms_make_an_environment_mixed() -> Outcome {
    // An external machine waiting for a container is neither a VM nor a container: the Docker
    // targets still render the environment.
    let text = "version: 1\nname: x\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  cache:\n    networks: { lab: 20 }\n    ports: { redis: 6379 }\n    app: { image: redis }\n  web:\n    networks: { lab: 10 }\n    depends_on: [cache]\noverrides:\n  external:\n    machines:\n      web: { ssh: { address: 10.9.0.10 } }\n";
    for target in [Target::DockerVm, Target::Hybrid] {
        assert!(build(text, target)?.requirements.is_empty(), "{target:?}");
    }
    Ok(())
}

#[test]
fn hybrid_forwards_a_published_tool() -> Outcome {
    let mut spec = mixed("")?.spec().clone();
    let tools = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    networks: { lab: 10 }\n    app: { image: alpine }\ntools:\n  web-ui: { image: \"nginx:1\", port: 80, publish: 9090 }\n";
    spec.tools = isoloom_spec::parse(tools).expect("parses").tools;
    let files = HybridBackend.render(&Environment::build(&spec, Target::Hybrid, None)).expect("renders");
    let compose = file(&files, "hybrid/compose.yml")?;
    assert!(compose.contains("    - 9090:80\n"), "{compose}");
    assert!(compose.contains("      isoloom-publish: null\n"), "{compose}");
    let forward = "m.vm.network \"forwarded_port\", guest: 9090, host: 9090, host_ip: \"127.0.0.1\"\n";
    assert!(file(&files, "hybrid/Vagrantfile")?.contains(forward));
    Ok(())
}
