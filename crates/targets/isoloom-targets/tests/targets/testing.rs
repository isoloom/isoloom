//! What the targets' tests share: the repository's examples as labs or plans, specs written in
//! a test, their problems, and the files a target generates, found by path.

use std::path::PathBuf;

use isoloom_plan::Environment;
use isoloom_spec::Spec;
use isoloom_spec::model::Target;
use isoloom_targets::{GeneratedFile, Lab, generate};

/// A test's outcome: a failure is the report.
pub(crate) type Outcome<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// The repository's examples folder.
pub(crate) fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../examples")
}

/// The authored spec of the repository's example `name`.
pub(crate) fn source(name: &str) -> Outcome<Spec> {
    Ok(isoloom_spec::load(&examples().join(name))?)
}

/// The repository's example `name`, as a lab.
pub(crate) fn example(name: &str) -> Outcome<Lab> {
    Ok(Lab::new(source(name)?))
}

/// A spec written in a test, as a lab (checked or not).
pub(crate) fn lab(yaml: &str) -> Outcome<Lab> {
    Ok(Lab::new(isoloom_spec::parse(yaml)?))
}

/// A spec written in a test, as a lab with no problem.
pub(crate) fn valid(yaml: &str) -> Outcome<Lab> {
    let lab = lab(yaml)?;
    match problems(&lab) {
        found if found.is_empty() => Ok(lab),
        found => Err(format!("the spec isn't valid: {found:?}").into()),
    }
}

/// A lab's problems, each as `path: reason`.
pub(crate) fn problems(lab: &Lab) -> Vec<String> {
    lab.problems().iter().map(isoloom_spec::syntax::Problem::line).collect()
}

/// The plan for `target` of the repository's example `name`, once it has no problem.
pub(crate) fn plan(name: &str, target: Target) -> Outcome<Environment> {
    let lab = example(name)?;
    match problems(&lab) {
        found if found.is_empty() => Ok(Environment::build(&lab.for_target(target), target, None)),
        found => Err(format!("example `{name}` isn't valid: {found:?}").into()),
    }
}

/// The text of the file among `files` whose path ends with `suffix`.
pub(crate) fn file<'a>(files: &'a [GeneratedFile], suffix: &str) -> Outcome<&'a str> {
    match files.iter().find(|f| f.path.ends_with(suffix)) {
        Some(f) => Ok(&f.contents),
        None => Err(format!("no {suffix} among {:?}", files.iter().map(|f| &f.path).collect::<Vec<_>>()).into()),
    }
}

/// The text of the file whose path ends with `suffix` among those `target` generates for `lab`.
pub(crate) fn generated(lab: &Lab, target: Target, suffix: &str) -> Outcome<String> {
    Ok(file(&generate(lab, target)?, suffix)?.to_owned())
}

/// The part of `text` from `from` up to (not including) the first `to` after it.
pub(crate) fn between<'a>(text: &'a str, from: &str, to: &str) -> Outcome<&'a str> {
    let (_, rest) = text.split_once(from).ok_or_else(|| format!("no `{from}` in:\n{text}"))?;
    let start = text.len() - rest.len() - from.len();
    let end = rest.find(to).ok_or_else(|| format!("no `{to}` after `{from}` in:\n{text}"))?;
    text.get(start..start + from.len() + end).ok_or_else(|| "not on a character boundary".into())
}

/// The part of `text` from the first `start` on.
pub(crate) fn from<'a>(text: &'a str, start: &str) -> Outcome<&'a str> {
    text.find(start)
        .and_then(|i| text.get(i..))
        .ok_or_else(|| format!("no `{start}` in:\n{text}").into())
}

/// Where `what` first is in `text`.
pub(crate) fn at(text: &str, what: &str) -> Outcome<usize> {
    text.find(what).ok_or_else(|| format!("`{what}` missing in:\n{text}").into())
}

/// The part of `text` before the first `end`.
pub(crate) fn before<'a>(text: &'a str, end: &str) -> Outcome<&'a str> {
    text.split_once(end)
        .map(|(head, _)| head)
        .ok_or_else(|| format!("no `{end}` in:\n{text}").into())
}

/// A one-network lab named `name`: `web` (a published HTTP port, on `web_os`) and `db` (Ubuntu),
/// each set up by a script.
pub(crate) fn web_and_db(name: &str, web_os: &str) -> String {
    format!(
        "version: 1
name: {name}
networks:
  lab: {{ cidr: 10.60.0.0/24 }}
machines:
  web:
    os: {web_os}
    networks: {{ lab: 10 }}
    ports: {{ web: {{ port: 80, protocol: http, publish: 8080 }} }}
    app: {{ steps: [{{ run: p.sh }}] }}
  db:
    os: ubuntu@24.04
    networks: {{ lab: 11 }}
    ports: {{ postgres: 5432 }}
    app: {{ steps: [{{ run: p.sh }}] }}
"
    )
}

/// `spec` with its Kali machines on Debian: the clouds have no Kali image yet.
pub(crate) fn kali_as_debian(spec: &mut Spec) -> Outcome {
    for m in spec.machines.values_mut() {
        if m.os.is_some_and(|os| os.name() == "kali") {
            m.os = Some("debian@12".parse()?);
        }
    }
    Ok(())
}
