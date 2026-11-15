//! The Compose backend's rendering of a few examples, as snapshots, and its determinism.

use std::error::Error;
use std::path::Path;

use isoloom_compose::ComposeBackend;
use isoloom_plan::Lab;
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::host_access::{Grants, HostAccess};
use isoloom_spec::model::Target;

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The files the Compose backend renders for `spec` (the authored format), which must be valid.
fn render(spec: isoloom_spec::Spec) -> Outcome<Vec<GeneratedFile>> {
    render_allowing(spec, Grants::none())
}

/// The files the Compose backend renders for `spec`, with whoever runs it allowing `grants`.
fn render_allowing(spec: isoloom_spec::Spec, grants: Grants) -> Outcome<Vec<GeneratedFile>> {
    let lab = Lab::new(spec).with_host_access(grants);
    let problems = lab.problems();
    if !problems.is_empty() {
        return Err(format!("the spec isn't valid: {problems:?}").into());
    }
    let docker = lab.for_target(Target::Docker);
    Ok(ComposeBackend::DOCKER.render(&lab.plan(&docker, Target::Docker, None)?)?)
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
fn a_build_with_a_published_image_emits_both_image_and_build() -> Outcome {
    let fixture = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/build-and-image.yml"))?;
    let rendered = compose(render(isoloom_spec::parse(&fixture)?)?)?;
    let parsed = isoloom_test_support::yaml(&rendered);
    assert_eq!(
        isoloom_test_support::yaml_str(&parsed, "services.web.image"),
        "ghcr.io/example/web:1",
        "the published image"
    );
    assert_eq!(
        isoloom_test_support::yaml_str(&parsed, "services.web.build.context"),
        "../../build/web",
        "the build block"
    );
    insta::assert_snapshot!(rendered);
    Ok(())
}

#[test]
fn a_machine_binds_folders_of_the_computer_that_runs_the_lab() -> Outcome {
    let fixture = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/host-folders.yml"))?;
    let allowed = Grants::of([HostAccess::HostFolders, HostAccess::WritableHostFolders]);
    insta::assert_snapshot!(compose(render_allowing(isoloom_spec::parse(&fixture)?, allowed)?)?);
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
    let env = lab.plan(&lab.for_target(Target::Docker), Target::Docker, None)?;
    let refused = ComposeBackend::DOCKER.render(&env).err().ok_or("rendered")?;
    assert!(matches!(refused, isoloom_render::Error::Refused { .. }), "{refused:?}");
    assert_eq!(
        refused.to_string(),
        "docker: environment-level provisioning (`provision:`) runs on the VM targets only"
    );
    Ok(())
}
