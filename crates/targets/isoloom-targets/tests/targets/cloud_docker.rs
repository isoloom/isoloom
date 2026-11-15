//! The `cloud-docker` target put together: the `docker` target's Compose file with a module per
//! cloud, and its refusals, Compose's capabilities first, then the clouds' own.

use isoloom_plan::Environment;
use isoloom_spec::model::Target;
use isoloom_targets::{Lab, backend};

use crate::testing::{Outcome, plan};

/// The example `name`, planned for `cloud-docker`.
fn env(name: &str) -> Outcome<Environment> {
    plan(name, Target::CloudDocker)
}

#[test]
fn the_compose_file_comes_first_then_a_module_per_cloud() -> Outcome {
    let files = backend(Target::CloudDocker).render(&env("hello-stack")?).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let compose = paths.iter().position(|p| *p == ".isoloom/docker/compose.yml").expect("the Compose file");
    let first_module = paths.iter().position(|p| p.starts_with(".isoloom/cloud-docker/")).expect("a module");
    assert!(compose < first_module, "{paths:?}");
    for cloud in isoloom_terraform::Cloud::ALL {
        assert!(paths.contains(&format!(".isoloom/cloud-docker/{}/main.tf", cloud.id()).as_str()), "{paths:?}");
    }
    Ok(())
}

#[test]
fn a_machine_needing_a_controller_is_refused_by_capabilities() -> Outcome {
    let e = env("ansible-pair")?;
    let why = backend(Target::CloudDocker).refusal(&e).expect("refused");
    assert!(why.contains("provision"), "{why}");
    assert!(backend(Target::CloudDocker).render(&e).is_err());
    Ok(())
}

#[test]
fn arm64_is_refused_by_the_clouds() {
    let lab = Lab::new(
        isoloom_spec::parse(
            "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    hardware: { arch: arm64 }\n    app: { image: nginx }\n",
        )
        .unwrap(),
    );
    let e = lab
        .plan(&lab.for_target(Target::CloudDocker), Target::CloudDocker, None)
        .expect("the spec plans");
    let why = backend(Target::CloudDocker).refusal(&e).unwrap();
    assert_eq!(
        why,
        "machine `w`: arm64, and every cloud's Docker host is x86-64 (its module picks x86-64 sizes and images)"
    );
    assert!(backend(Target::CloudDocker).render(&e).unwrap_err().to_string().ends_with(&why));
}

#[test]
fn containers_needing_more_than_the_largest_host_are_refused_not_squeezed() {
    let env = |db_mb: u32| {
        let lab = Lab::new(
            isoloom_spec::parse(&format!(
                "version: 1\nname: a\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  web:\n    networks: {{ lab: 10 }}\n    app: {{ image: nginx }}\n  db:\n    networks: {{ lab: 11 }}\n    resources: {{ memory_mb: {db_mb} }}\n    app: {{ image: postgres }}\n"
            ))
            .unwrap(),
        );
        lab.plan(&lab.for_target(Target::CloudDocker), Target::CloudDocker, None)
            .expect("the spec plans")
    };
    // 512 MB + 14 GB + Docker's 1 GB: 15.5 GB fits the 16 GB host.
    assert_eq!(backend(Target::CloudDocker).refusal(&env(14336)), None);
    // 512 MB + 16 GB + 1 GB was given the 16 GB host all the same.
    let why = backend(Target::CloudDocker).refusal(&env(16384)).unwrap();
    assert!(
        why.starts_with(
            "no cloud can run it (aws, azure, gcp, oci: the containers' host needs 17920 MB of memory (Docker's 1024 MB and the containers', machine `db` asking for the most, 16384 MB): the largest Docker host here has 16384 MB; digitalocean: "
        ),
        "{why}"
    );
    assert!(backend(Target::CloudDocker).render(&env(16384)).unwrap_err().to_string().ends_with(&why));
}

#[test]
fn linodes_root_host_brings_the_project_in_without_sudo_the_others_through_it() -> Outcome {
    // The module brings the project in the same way: as root on Linode, through `sudo` elsewhere.
    let files = backend(Target::CloudDocker).render(&env("configured-app")?).unwrap();
    let module = |cloud: &str| crate::testing::file(&files, &format!("cloud-docker/{cloud}/main.tf")).map(str::to_owned);
    assert!(module("aws")?.contains("sudo sh -c 'rm -rf /opt/isoloom"));
    let linode_module = module("linode")?;
    assert!(
        linode_module.contains("\"sh -c 'rm -rf /opt/isoloom") && !linode_module.contains("sudo "),
        "{linode_module}"
    );
    Ok(())
}
