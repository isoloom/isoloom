//! The Compose file of specs written in the new syntax, through lowering: a secret delivered as a
//! file from its input (and its value to the steps that need it), and an image built from
//! portable steps.

use std::error::Error;
use std::path::Path;

use isoloom_compose::ComposeBackend;
use isoloom_plan::Environment;
use isoloom_plan::lower::{Scope, lower};
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The Docker plan of a design example of isoloom-plan's `tests/spec-examples`.
fn plan(name: &str) -> Outcome<Environment> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../core/isoloom-plan/tests/spec-examples")
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

/// The `up` lifecycle of the Docker run manifest, as text. In a host Compose manifest `--build`
/// appears only in the `up` step and `--ignore-pull-failures` only in the pull step, so a run's
/// pull-or-build behaviour is readable from this text.
fn up_lifecycle_text(env: &Environment) -> Outcome<String> {
    let backend = isoloom_compose::ComposeBackend::DOCKER;
    let manifest = isoloom_render::backend::Backend::run_manifest(&backend, env)?.ok_or("no run manifest")?;
    Ok(format!("{:?}", manifest.lifecycle.up))
}

/// The argv of each `down` step of `env`'s Docker run manifest, by step id, as text.
fn down_steps(env: &Environment) -> Outcome<Vec<(String, String)>> {
    let backend = isoloom_compose::ComposeBackend::DOCKER;
    let manifest = isoloom_render::backend::Backend::run_manifest(&backend, env)?.ok_or("no run manifest")?;
    Ok(manifest.lifecycle.down.iter().map(|s| (s.id.to_string(), format!("{s:?}"))).collect())
}

/// Regression (the Cyber CTF rig's ubuntu2404 e2e:lab, six anonymous volumes left): an
/// environment that keeps a volume (`persistent: true`) went down without `--volumes`, so the
/// persistent one survived, and so did every container's anonymous volume (an image's own
/// `VOLUME`, MySQL's data directory), one more each start and stop. The containers now go first
/// with their anonymous volumes (`rm --volumes`, which never takes a named one), the persistent
/// volume is named by no step, and an environment that keeps nothing still goes `down --volumes`
/// in one step.
#[test]
fn a_lab_that_keeps_a_volume_removes_its_containers_anonymous_volumes_but_never_the_kept_one() -> Outcome {
    let kept = plan_of(
        "version: 1\nname: case-file\nnetworks: { app: { cidr: 10.70.0.0/24 } }\nvolumes:\n  evidence: { owner: database, persistent: true }\n  scratch: { owner: database }\nmachines:\n  database: { networks: { app: 5 }, app: { image: mysql }, volumes: { evidence: /srv/evidence, scratch: /srv/scratch } }\n",
    )?;
    let steps = down_steps(&kept)?;
    let ids: Vec<&str> = steps.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids[..2], ["anonymous-volumes", "down"], "{steps:?}");
    let (_, rm) = &steps[0];
    assert!(
        rm.contains("\"rm\"") && rm.contains("\"--stop\"") && rm.contains("\"--force\"") && rm.contains("\"--volumes\""),
        "{rm}"
    );
    let (_, down) = &steps[1];
    assert!(!down.contains("\"--volumes\""), "the kept volume survives the down: {down}");
    let named: Vec<&String> = steps.iter().map(|(_, s)| s).filter(|s| s.contains("evidence")).collect();
    assert!(named.is_empty(), "no step names the kept volume: {named:?}");
    assert!(
        steps.iter().any(|(id, s)| id == "volumes" && s.contains("scratch")),
        "the environment's own still goes: {steps:?}"
    );

    let plain = plan_of(
        "version: 1\nname: plain\nnetworks: { app: { cidr: 10.70.0.0/24 } }\nmachines:\n  database: { networks: { app: 5 }, app: { image: mysql } }\n",
    )?;
    let once = down_steps(&plain)?;
    assert_eq!(once[0].0, "down", "{once:?}");
    assert!(once[0].1.contains("\"--volumes\""), "{once:?}");
    assert!(!once.iter().any(|(id, _)| id == "anonymous-volumes"), "{once:?}");
    Ok(())
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
    let text = serde_yaml_ng::to_string(&c)?;
    for (name, service) in services(&c) {
        let env = serde_yaml_ng::to_string(service.get("environment").unwrap_or(&serde_yaml_ng::Value::Null))?;
        assert!(!env.contains("${DEMO_SECRET"), "{name}'s environment: {env}");
    }
    assert!(text.contains("isoloom.port.site"), "{text}");
    Ok(())
}

/// Compose writes the file again from the variable each time it starts the container, and won't
/// start it without the variable: the value goes to `start` (resume) as well as `up`.
#[test]
fn an_input_file_reaches_the_steps_that_create_and_start_its_machine() -> Outcome {
    let backend = isoloom_compose::ComposeBackend::DOCKER;
    let manifest = isoloom_render::backend::Backend::run_manifest(&backend, &plan("help-desk")?)?.ok_or("no run manifest")?;
    let slot = manifest
        .secrets
        .iter()
        .find(|s| s.name.to_string() == "DEMO_SECRET")
        .ok_or("no DEMO_SECRET slot")?;
    let [
        isoloom_manifest::Delivery::Env {
            var,
            steps: isoloom_manifest::StepSelection::Only(steps),
        },
    ] = slot.deliver.as_slice()
    else {
        return Err(format!("{:?}", slot.deliver).into());
    };
    assert_eq!(var.to_string(), "DEMO_SECRET");
    assert_eq!(steps.iter().map(ToString::to_string).collect::<Vec<_>>(), ["up", "start"]);
    Ok(())
}

/// A machine that publishes an image it also builds (the Compose build+image combo) makes the run
/// pull the published image first; `up` then drops `--build`, so a user runs the published
/// image and only an image that could not be pulled is built. An environment that only builds locally keeps
/// `--build` and gains no pull step, so an author's edits always rebuild.
#[test]
fn a_published_build_pulls_before_up_but_a_local_build_still_forces_a_build() -> Outcome {
    let combo = up_lifecycle_text(&plan_of(
        "version: 1\nname: lab\nmachines:\n  web:\n    app:\n      image: ghcr.io/example/lab-web:1\n      build: { context: web }\n",
    )?)?;
    assert!(combo.contains("--ignore-pull-failures"), "a published build pulls first: {combo}");
    assert!(!combo.contains("--build"), "a published build must not force a local build: {combo}");

    let local = up_lifecycle_text(&plan_of("version: 1\nname: lab\nmachines:\n  web:\n    app:\n      build: { context: web }\n")?)?;
    assert!(!local.contains("--ignore-pull-failures"), "a local build gains no pull step: {local}");
    assert!(local.contains("--build"), "a local build still forces a build: {local}");
    Ok(())
}

#[test]
fn portable_steps_build_from_the_os_image() -> Outcome {
    let files = render("legacy-intranet")?;
    let dockerfile = file(&files, ".isoloom/docker/build/portal/Dockerfile")?;
    assert!(dockerfile.contains("FROM debian:12@sha256:"));
    assert!(dockerfile.contains("COPY --chown=www-data [\"portal/\", \"/var/www/html/\"]\n"));
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
