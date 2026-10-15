//! The `cloud-docker` target put together: the `docker` target's Compose file with a module per
//! cloud, and its refusals, Compose's capabilities first, then the clouds' own.

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_targets::{CloudDockerBackend, Lab};

use crate::testing::{Outcome, plan};

/// The example `name`, planned for `cloud-docker`.
fn env(name: &str) -> Outcome<Environment> {
    plan(name, Target::CloudDocker)
}

#[test]
fn the_compose_file_comes_first_then_a_module_per_cloud() -> Outcome {
    let files = CloudDockerBackend.render(&env("hello-stack")?).unwrap();
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
    let why = CloudDockerBackend.refusal(&e).expect("refused");
    assert!(why.contains("provision"), "{why}");
    assert!(CloudDockerBackend.render(&e).is_err());
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
    let e = Environment::build(&lab.for_target(Target::CloudDocker), Target::CloudDocker, None);
    let why = CloudDockerBackend.refusal(&e).unwrap();
    assert_eq!(why, "machine `w`: arm64 on a cloud VM comes later (Graviton instances and arm64 images)");
    assert!(CloudDockerBackend.render(&e).unwrap_err().to_string().ends_with(&why));
}
