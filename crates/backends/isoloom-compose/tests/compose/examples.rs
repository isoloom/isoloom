//! The Compose files of a few examples and fixtures, as snapshots, and their determinism.

use std::path::Path;

use isoloom_compose::ComposeBackend;
use isoloom_plan::Lab;
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::host_access::{Grants, HostAccess};
use isoloom_spec::model::Target;

use crate::testing::Outcome;

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
    Ok(())
}

#[test]
fn a_machine_binds_folders_of_the_computer_that_runs_the_lab() -> Outcome {
    let fixture = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/host-folders.yml"))?;
    let allowed = Grants::of([HostAccess::HostFolders, HostAccess::WritableHostFolders]);
    insta::assert_snapshot!(compose(render_allowing(isoloom_spec::parse(&fixture)?, allowed)?)?);
    Ok(())
}

/// The examples whose Compose file a reviewer reads whole, each its snapshot: a stack of
/// services, networks routed through a router, a volume shared, a gateway machine.
#[test]
fn each_examples_compose_file_is_its_snapshot() -> Outcome {
    for (snapshot, name) in [
        ("hello_stack", "hello-stack"),
        ("segmented_routes_through_the_router", "segmented"),
        ("shared_files_share_one_volume", "shared-files"),
        ("edge_firewall_gateway_machine", "edge-firewall"),
    ] {
        insta::assert_snapshot!(snapshot, compose(example(name)?)?);
    }
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
fn rendering_twice_gives_the_same_files() -> Outcome {
    for name in ["hello-stack", "segmented", "vlan-office", "cisco-iol"] {
        assert_eq!(example(name)?, example(name)?, "{name}");
    }
    Ok(())
}
