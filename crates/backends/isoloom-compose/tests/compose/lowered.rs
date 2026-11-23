//! The Compose file of specs written in the new syntax, through lowering: a secret delivered as a
//! file from its input (and its value to the steps that need it), and an image built from
//! portable steps.

use std::path::Path;

use isoloom_compose::ComposeBackend;
use isoloom_plan::Environment;
use isoloom_plan::lower::{Scope, lower};
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

use crate::testing::Outcome;

/// The Docker plan of a design example of isoloom-plan's `tests/fixtures/specs`.
fn plan(name: &str) -> Outcome<Environment> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../core/isoloom-plan/tests/fixtures/specs")
        .join(name)
        .join("isoloom.yml");
    let source = isoloom_spec::syntax::parse(&std::fs::read_to_string(path)?).map_err(|problems| format!("{name} doesn't parse: {problems:?}"))?;
    let lab = isoloom_plan::Lab::new(source.clone());
    let lowered = lower(&source, lab.catalogue(), Scope::Target(Target::Docker));
    Ok(lab.plan(&lowered.spec, Target::Docker, None)?)
}

/// The files Compose renders for a design example.
fn render(name: &str) -> Outcome<Vec<GeneratedFile>> {
    Ok(ComposeBackend::DOCKER.render(&plan(name)?)?)
}

/// The Docker plan of an inline spec.
fn plan_of(source: &str) -> Outcome<Environment> {
    let parsed = isoloom_spec::syntax::parse(source).map_err(|p| format!("doesn't parse: {p:?}"))?;
    let lab = isoloom_plan::Lab::new(parsed.clone());
    let lowered = lower(&parsed, lab.catalogue(), Scope::Target(Target::Docker));
    Ok(lab.plan(&lowered.spec, Target::Docker, None)?)
}

/// The text of the file at `path` among `files`.
fn file<'a>(files: &'a [GeneratedFile], path: &str) -> Outcome<&'a str> {
    let found = files.iter().find(|f| f.path == path).ok_or_else(|| format!("no {path}"))?;
    Ok(&found.contents)
}

/// The parsed Compose file, and its services as (name, definition).
fn compose(files: &[GeneratedFile]) -> Outcome<serde_yaml_ng::Value> {
    Ok(serde_yaml_ng::from_str(file(files, ".isoloom/docker/compose.yml")?)?)
}

fn services(c: &serde_yaml_ng::Value) -> Vec<(&str, &serde_yaml_ng::Value)> {
    c.get("services")
        .and_then(serde_yaml_ng::Value::as_mapping)
        .into_iter()
        .flatten()
        .filter_map(|(name, service)| Some((name.as_str()?, service)))
        .collect()
}

#[test]
fn an_input_file_is_a_secret_read_from_its_input_never_the_environment() -> Outcome {
    let c = compose(&render("help-desk")?)?;
    // The secret is read from the input's variable...
    assert_eq!(
        c.get("secrets")
            .and_then(|s| s.get("input-demo-secret"))
            .and_then(|s| s.get("environment"))
            .and_then(|e| e.as_str()),
        Some("DEMO_SECRET"),
        "{c:?}"
    );
    // ...mounted as the file, owned and private...
    let mounts: Vec<&serde_yaml_ng::Value> = services(&c)
        .into_iter()
        .flat_map(|(_, s)| s.get("secrets").and_then(|l| l.as_sequence()).into_iter().flatten())
        .filter(|m| m.get("source").and_then(|v| v.as_str()) == Some("input-demo-secret"))
        .collect();
    let mount = match mounts.as_slice() {
        [one] => *one,
        _ => return Err(format!("one service mounts the secret, not {}: {c:?}", mounts.len()).into()),
    };
    let field = |k: &str| mount.get(k).and_then(|v| v.as_str());
    assert_eq!(
        (field("target"), field("uid"), field("mode")),
        (Some("/srv/archive/secret.txt"), Some("1000"), Some("0400"))
    );
    // ...and never put in a service's environment.
    for (name, service) in services(&c) {
        let env = serde_yaml_ng::to_string(service.get("environment").unwrap_or(&serde_yaml_ng::Value::Null))?;
        assert!(!env.contains("${DEMO_SECRET"), "{name}'s environment: {env}");
    }
    Ok(())
}

/// A machine that publishes an image it also builds (the Compose build+image combo) makes the run
/// pull the published image first; `up` then drops `--build`, so a user runs the published image
/// and only an image that could not be pulled is built. An environment that only builds locally
/// keeps `--build` and gains no pull step, so an author's edits always rebuild.
#[test]
fn a_published_build_pulls_before_up_but_a_local_build_still_forces_a_build() -> Outcome {
    // The start's commands: the pulls, then the one `up`.
    let start = |spec: &str| -> Outcome<(Vec<String>, String)> {
        let mut commands = isoloom_compose::vm_start(&plan_of(spec)?)?.start;
        let up = commands.pop().ok_or("no command")?;
        assert!(up.contains(" up --detach "), "the last command is `up`: {up}");
        Ok((commands, up))
    };
    let (published_pulls, published_up) =
        start("version: 1\nname: lab\nmachines:\n  web:\n    app:\n      image: ghcr.io/example/lab-web:1\n      build: { context: web }\n")?;
    assert!(
        matches!(published_pulls.as_slice(), [pull] if pull.contains(" pull --ignore-pull-failures")),
        "a published build pulls first: {published_pulls:?}"
    );
    assert!(
        !published_up.contains("--build"),
        "a published build must not force a local build: {published_up}"
    );
    let (local_pulls, local_up) = start("version: 1\nname: lab\nmachines:\n  web:\n    app:\n      build: { context: web }\n")?;
    assert_eq!(local_pulls, Vec::<String>::new(), "a local build gains no pull step");
    assert!(
        local_up.contains(" up --detach --build ") && local_up.contains(" --wait"),
        "a local build still forces a build: {local_up}"
    );
    Ok(())
}

#[test]
fn portable_steps_build_from_the_os_image() -> Outcome {
    let files = render("legacy-intranet")?;
    let dockerfile = file(&files, ".isoloom/docker/build/portal/Dockerfile")?;
    // Its instructions, the comments left out: the first is the pinned OS image.
    let lines: Vec<&str> = dockerfile.lines().filter(|l| !l.starts_with('#')).collect();
    assert!(
        lines.first().is_some_and(|from| from.starts_with("FROM debian:12@sha256:")),
        "the OS image, pinned: {dockerfile}"
    );
    assert!(lines.contains(&"COPY --chown=www-data [\"portal/\", \"/var/www/html/\"]"), "{dockerfile}");
    let c = compose(&files)?;
    let portal = services(&c)
        .into_iter()
        .find(|(name, _)| *name == "portal")
        .map(|(_, s)| s)
        .ok_or("no portal")?;
    let build = |k: &str| portal.get("build").and_then(|b| b.get(k)).and_then(|v| v.as_str());
    assert_eq!(
        (build("context"), build("dockerfile")),
        (Some("../.."), Some(".isoloom/docker/build/portal/Dockerfile"))
    );
    // The clerk is the access box: its OS image is the parameterized default (the runner
    // overrides it with one it is given), and it sits in the `access` profile so the
    // initial `up` skips it, brought up on demand.
    let clerk = services(&c).into_iter().find(|(name, _)| *name == "clerk").map(|(_, s)| s).ok_or("no clerk")?;
    let image = clerk.get("image").and_then(|v| v.as_str()).unwrap_or_default();
    assert!(image.starts_with("${ISOLOOM_ACCESS_IMAGE:-debian:12@sha256:"), "{image}");
    let profiles: Vec<&str> = clerk
        .get("profiles")
        .and_then(|p| p.as_sequence())
        .into_iter()
        .flatten()
        .filter_map(|p| p.as_str())
        .collect();
    assert_eq!(profiles, ["access"]);
    Ok(())
}
