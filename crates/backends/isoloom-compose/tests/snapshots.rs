//! The Compose backend's rendering of a few examples, as snapshots, and its determinism.

use std::error::Error;
use std::path::Path;

use isoloom_plan::{Environment, Lab};
use isoloom_render::GeneratedFile;
use isoloom_spec::model::Target;

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The files the Compose backend renders for `spec` (the authored format), which must be valid.
fn render(spec: isoloom_spec::Spec) -> Outcome<Vec<GeneratedFile>> {
    let lab = Lab::new(spec);
    let problems = lab.problems();
    if !problems.is_empty() {
        return Err(format!("the spec isn't valid: {problems:?}").into());
    }
    let docker = lab.for_target(Target::Docker);
    Ok(isoloom_compose::render(Target::Docker, &Environment::build(&docker, Target::Docker, None))?)
}

/// An example of the project, as written.
fn load(name: &str) -> Outcome<isoloom_spec::Spec> {
    Ok(isoloom_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name))?)
}

/// The files the Compose backend renders for an example of the project.
fn example(name: &str) -> Outcome<Vec<GeneratedFile>> {
    render(load(name)?)
}

/// The Compose file among `files`.
fn compose(files: Vec<GeneratedFile>) -> Outcome<String> {
    let file = files.into_iter().find(|f| f.path.ends_with("compose.yml")).ok_or("no Compose file")?;
    Ok(file.contents)
}

#[test]
fn a_machine_runs_its_own_process() -> Outcome {
    let fixture = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/own-process.yml"))?;
    insta::assert_snapshot!(compose(render(isoloom_spec::parse(&fixture)?)?)?);
    Ok(())
}

#[test]
fn hello_stack() -> Outcome {
    insta::assert_snapshot!(compose(example("hello-stack")?)?);
    Ok(())
}

#[test]
fn segmented_routes_through_the_router() -> Outcome {
    insta::assert_snapshot!(compose(example("segmented")?)?);
    Ok(())
}

#[test]
fn shared_files_share_one_volume() -> Outcome {
    insta::assert_snapshot!(compose(example("shared-files")?)?);
    Ok(())
}

#[test]
fn tuned_web_settings() -> Outcome {
    // Its TPM makes it a VM only: without it, the same machine's container form and settings.
    let mut spec = load("tuned-web")?;
    let web = spec.machines.get_mut("web").ok_or("no web machine")?;
    web.hardware.tpm = isoloom_spec::syntax::Tpm::Auto;
    insta::assert_snapshot!(compose(render(spec)?)?);
    Ok(())
}

#[test]
fn edge_firewall_gateway_machine() -> Outcome {
    insta::assert_snapshot!(compose(example("edge-firewall")?)?);
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for name in ["hello-stack", "segmented", "vlan-office", "cisco-iol"] {
        assert_eq!(example(name)?, example(name)?, "{name}");
    }
    Ok(())
}

#[test]
fn provisioning_is_refused_without_rendering() -> Outcome {
    let lab = Lab::parse(
        "version: 1\nname: p\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    networks: { lab: 10 }\n    os: debian@12\n    app: { image: alpine }\nprovision:\n  - ansible: site.yml\n",
    )
    .map_err(|problems| format!("{problems:?}"))?;
    let env = Environment::build(&lab.for_target(Target::Docker), Target::Docker, None);
    let refused = isoloom_compose::render(Target::Docker, &env).err().ok_or("rendered")?;
    assert!(matches!(refused, isoloom_compose::Error::Refused { .. }), "{refused:?}");
    assert_eq!(
        refused.to_string(),
        "docker: environment-level provisioning (`provision:`) runs on VM targets for now"
    );
    Ok(())
}
