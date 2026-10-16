//! The Compose file of specs written in the new syntax, through lowering: a flag delivered as a
//! file from its input (and its value to the steps that need it), and an image built from
//! portable steps.

use std::error::Error;
use std::path::Path;

use isoloom_plan::Environment;
use isoloom_plan::lower::{Scope, lower};
use isoloom_render::GeneratedFile;
use isoloom_spec::model::Target;

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The Docker plan of a design example of `plan/phase2/spec-examples`.
fn plan(name: &str) -> Outcome<Environment> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../plan/phase2/spec-examples")
        .join(name)
        .join("isoloom.yml");
    let source = isoloom_spec::syntax::parse(&std::fs::read_to_string(path)?).map_err(|problems| format!("{name} doesn't parse: {problems:?}"))?;
    let lowered = lower(&source, isoloom_plan::Lab::new(source.clone()).catalogue(), Scope::Target(Target::Docker));
    Ok(Environment::build(&lowered.spec, Target::Docker, None))
}

/// The files Compose renders for a design example.
fn render(name: &str) -> Outcome<Vec<GeneratedFile>> {
    Ok(isoloom_compose::render(Target::Docker, &plan(name)?)?)
}

/// The text of the file at `path` among `files`.
fn file<'a>(files: &'a [GeneratedFile], path: &str) -> Outcome<&'a str> {
    let found = files.iter().find(|f| f.path == path).ok_or_else(|| format!("no {path}"))?;
    Ok(&found.contents)
}

#[test]
fn a_flag_is_a_secret_file_read_from_its_input_never_the_environment() -> Outcome {
    let files = render("help-desk")?;
    let c = file(&files, ".isoloom/docker/compose.yml")?;
    assert!(c.contains("secrets:\n  input-flag:\n    environment: FLAG\n"), "{c}");
    assert!(
        c.contains("- source: input-flag\n      target: /srv/archive/flag.txt\n      uid: '1000'\n      mode: '0400'"),
        "{c}"
    );
    assert!(!c.contains("FLAG: ${FLAG"), "{c}");
    assert!(c.contains("isoloom.port.site"), "{c}");
    Ok(())
}

/// Compose writes the file again from the variable each time it starts the container, and won't
/// start it without the variable: the value goes to `start` (resume) as well as `up`.
#[test]
fn a_flag_reaches_the_steps_that_create_and_start_its_machine() -> Outcome {
    let backend = isoloom_compose::ComposeBackend(Target::Docker);
    let manifest = isoloom_render::backend::Backend::run_manifest(&backend, &plan("help-desk")?)?.ok_or("no run manifest")?;
    let slot = manifest.secrets.iter().find(|s| s.name.to_string() == "FLAG").ok_or("no FLAG slot")?;
    let [
        isoloom_manifest::Delivery::Env {
            var,
            steps: isoloom_manifest::StepSelection::Only(steps),
        },
    ] = slot.deliver.as_slice()
    else {
        return Err(format!("{:?}", slot.deliver).into());
    };
    assert_eq!(var.to_string(), "FLAG");
    assert_eq!(steps.iter().map(ToString::to_string).collect::<Vec<_>>(), ["up", "start"]);
    Ok(())
}

#[test]
fn portable_steps_build_from_the_os_image() -> Outcome {
    let files = render("legacy-intranet")?;
    let dockerfile = file(&files, ".isoloom/docker/build/portal/Dockerfile")?;
    assert!(dockerfile.contains("FROM debian:12@sha256:"));
    assert!(dockerfile.contains("COPY --chown=www-data portal/ /var/www/html/\n"));
    let c = file(&files, ".isoloom/docker/compose.yml")?;
    assert!(c.contains("context: ../..\n      dockerfile: .isoloom/docker/build/portal/Dockerfile"), "{c}");
    // The clerk is its OS alone: the OS's image, pinned, kept running idle.
    assert!(c.contains("image: debian:12@sha256:"), "{c}");
    Ok(())
}
