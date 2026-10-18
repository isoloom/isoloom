//! The `hybrid` target: the containers beside the VMs, offered only when the environment mixes
//! them.

use isoloom_spec::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, example, file};

#[test]
fn hybrid_runs_containers_beside_the_vms() -> Outcome {
    let spec = example("mixed-office")?;
    let files = generate(&spec, Target::Hybrid)?;
    let vagrantfile = file(&files, "hybrid/Vagrantfile")?;
    let compose = file(&files, "hybrid/compose.yml")?;
    // The Windows server is a VM; the web app is a container, not a VM.
    assert!(vagrantfile.contains("config.vm.define \"files01\""));
    assert!(!vagrantfile.contains("config.vm.define \"intranet\""));
    assert!(vagrantfile.contains("config.vm.define \"isoloom-docker\""));
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
