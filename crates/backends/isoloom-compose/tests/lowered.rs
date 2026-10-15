//! The Compose file of specs written in the new syntax, through lowering: a flag delivered as a
//! file from its input, and an image built from portable steps.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_plan::lower::{Scope, lower};
use isoloom_spec::model::Target;

/// The files Compose renders for a design example of `plan/phase2/spec-examples`.
fn render(name: &str) -> Vec<isoloom_spec::files::GeneratedFile> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../plan/phase2/spec-examples")
        .join(name)
        .join("isoloom.yml");
    let source = isoloom_spec::syntax::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let lowered = lower(&source, Scope::Target(Target::Docker));
    isoloom_compose::render(Target::Docker, &Environment::build(&lowered.spec, Target::Docker, None)).unwrap()
}

fn compose(files: &[isoloom_spec::files::GeneratedFile]) -> &str {
    &files.iter().find(|f| f.path.ends_with("compose.yml")).unwrap().contents
}

#[test]
fn a_flag_is_a_secret_file_read_from_its_input_never_the_environment() {
    let files = render("help-desk");
    let c = compose(&files);
    assert!(c.contains("secrets:\n  input-flag:\n    environment: FLAG\n"), "{c}");
    assert!(
        c.contains("- source: input-flag\n      target: /srv/archive/flag.txt\n      uid: '1000'\n      mode: '0400'"),
        "{c}"
    );
    assert!(!c.contains("FLAG: ${FLAG"), "{c}");
    assert!(c.contains("isoloom.port.site"), "{c}");
}

#[test]
fn portable_steps_build_from_the_os_image() {
    let files = render("legacy-intranet");
    let dockerfile = files
        .iter()
        .find(|f| f.path == ".isoloom/docker/build/portal/Dockerfile")
        .expect("the portal's Dockerfile");
    assert!(dockerfile.contents.contains("FROM debian:12@sha256:"));
    assert!(dockerfile.contents.contains("COPY --chown=www-data portal/ /var/www/html/\n"));
    let c = compose(&files);
    assert!(c.contains("context: ../..\n      dockerfile: .isoloom/docker/build/portal/Dockerfile"), "{c}");
    // The clerk is its OS alone: the OS's image, pinned, kept running idle.
    assert!(c.contains("image: debian:12@sha256:"), "{c}");
}
