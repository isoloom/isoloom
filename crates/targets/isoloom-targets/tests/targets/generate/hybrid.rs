//! The `hybrid` target: the containers beside the VMs, offered only when the environment mixes
//! them.

use isoloom_spec::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, example, file, lab};

#[test]
fn hybrid_runs_containers_beside_the_vms() -> Outcome {
    let spec = example("mixed-office")?;
    let files = generate(&spec, Target::Hybrid)?;
    let vagrantfile = file(&files, "hybrid/Vagrantfile")?;
    let compose = file(&files, "hybrid/compose.yml")?;
    // The Windows server is a VM; the web app is a container, not a VM.
    assert!(vagrantfile.contains("config.vm.define \"files01\""));
    assert!(!vagrantfile.contains("config.vm.define \"intranet\""));
    assert!(vagrantfile.contains("config.vm.define \"isoloom-docker-host\""));
    assert!(vagrantfile.contains("--nicpromisc2"));
    assert!(vagrantfile.contains("docker network create -d macvlan --subnet 192.168.58.0/24"));
    assert!(vagrantfile.contains("guest: 8083, host: 8083, host_ip: \"127.0.0.1\""));
    // The containers join the VMs' network at their own address, and know the VMs by name.
    assert!(compose.contains("ipv4_address: 192.168.58.20"));
    assert!(compose.contains("name: isoloom-mixed-office-office"));
    assert!(compose.contains("files01:192.168.58.10"));
    assert!(!compose.contains("isoloom-check"));
    Ok(())
}

#[test]
fn hybrid_is_offered_only_for_mixed_environments() -> Outcome {
    assert!(!example("hello-stack")?.possible().contains(&Target::Hybrid));
    assert!(example("mixed-office")?.possible().contains(&Target::Hybrid));
    Ok(())
}

/// A container's set-up job nothing waits for exits once done, and Compose's `up --wait` fails
/// on any service that exits: the host waits for the rest, then runs that job attached.
#[test]
fn the_host_runs_a_leaf_set_up_job_after_the_wait() -> Outcome {
    let spec = lab(r"
version: 1
name: leafy
networks: { lan: { cidr: 10.72.0.0/24 } }
machines:
  srv: { os: windows-server@2019, networks: { lan: 10 } }
  web: { networks: { lan: 20 }, ports: { web: 80 }, app: { image: nginx }, setup: [seed.sh] }
")?;
    let files = generate(&spec, Target::Hybrid)?;
    let vagrantfile = file(&files, "hybrid/Vagrantfile")?;
    let dc = "docker compose -f .isoloom/hybrid/compose.yml";
    let start = format!(
        "{dc} up -d --build --wait --wait-timeout 900 web isoloom-probe-amd64 && {dc} up --no-deps --exit-code-from isoloom-init-web-1 isoloom-init-web-1"
    );
    assert!(vagrantfile.contains(&start), "{vagrantfile}");
    Ok(())
}

/// The containers' host is the Docker host docker-vm's VM is: the same set-up under the same
/// names, published ports reaching every container included.
#[test]
fn the_containers_host_is_set_up_as_docker_vm_s_vm() -> Outcome {
    let hybrid = file(&generate(&example("mixed-office")?, Target::Hybrid)?, "hybrid/Vagrantfile")?.to_owned();
    let docker_vm = file(&generate(&example("hello-stack")?, Target::DockerVm)?, "docker-vm/Vagrantfile")?.to_owned();
    for (name, kind) in [("project", "isoloom_project"), ("docker", "shell")] {
        assert!(hybrid.contains(&format!("m.vm.provision \"{name}\", type: \"{kind}\"")), "{name}: {hybrid}");
        assert!(
            docker_vm.contains(&format!("config.vm.provision \"{name}\", type: \"{kind}\"")),
            "{name}: {docker_vm}"
        );
    }
    for v in [&hybrid, &docker_vm] {
        assert!(v.contains("isoloom-published.service"), "{v}");
    }
    assert!(hybrid.contains("m.vm.box = \"bento/debian-12\""), "{hybrid}");
    Ok(())
}
