//! What the targets' tests share: the repository's examples as environments or plans, specs written in
//! a test, their problems, and the files a target generates, found by path.

use std::path::PathBuf;

use isoloom_plan::Environment;
use isoloom_spec::Spec;
use isoloom_spec::host_access::{Grants, HostAccess};
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

/// The repository's example `name`, as an environment.
pub(crate) fn example(name: &str) -> Outcome<Lab> {
    Ok(Lab::new(source(name)?))
}

/// A spec written in a test, as an environment (checked or not).
pub(crate) fn lab(yaml: &str) -> Outcome<Lab> {
    Ok(Lab::new(isoloom_spec::parse(yaml)?))
}

/// A spec written in a test, as an environment with no problem.
pub(crate) fn valid(yaml: &str) -> Outcome<Lab> {
    no_problem(lab(yaml)?)
}

/// `lab` with every access to the computer that runs it allowed, as `--allow-host` allows each:
/// the tests of what a target does with a field that reaches that computer.
pub(crate) fn allowed(lab: Lab) -> Lab {
    lab.with_host_access(Grants::of(HostAccess::ALL))
}

/// A spec written in a test that reaches the computer that runs it, as an environment allowed
/// to, with no problem.
pub(crate) fn valid_allowed(yaml: &str) -> Outcome<Lab> {
    no_problem(allowed(lab(yaml)?))
}

/// `lab`, once it has no problem.
fn no_problem(lab: Lab) -> Outcome<Lab> {
    match problems(&lab) {
        found if found.is_empty() => Ok(lab),
        found => Err(format!("the spec isn't valid: {found:?}").into()),
    }
}

/// An environment's problems, each as `path: reason`.
pub(crate) fn problems(lab: &Lab) -> Vec<String> {
    lab.problems().iter().map(isoloom_spec::syntax::Problem::line).collect()
}

/// The plan for `target` of the repository's example `name`, once it has no problem.
pub(crate) fn plan(name: &str, target: Target) -> Outcome<Environment> {
    let lab = example(name)?;
    match problems(&lab) {
        found if found.is_empty() => Ok(lab.plan(&lab.for_target(target), target, None)?),
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

/// A one-network environment named `name`: `web` (a published HTTP port, on `web_os`) and `db` (Ubuntu),
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

/// The Vagrantfile `lab` generates, read into its Ruby blocks.
pub(crate) fn vagrantfile(lab: &Lab) -> Outcome<isoloom_test_support::RubyBlock> {
    Ok(isoloom_test_support::ruby_blocks(&generated(
        lab,
        Target::Vagrant,
        ".isoloom/vagrant/Vagrantfile",
    )?))
}

/// The VMs a Vagrantfile defines (`config.vm.define "<name>"`), in its order.
pub(crate) fn defined(vf: &isoloom_test_support::RubyBlock) -> Vec<String> {
    vf.find("config.vm.define \"")
        .iter()
        .filter_map(|b| b.head.strip_prefix("config.vm.define \"").and_then(|r| r.split('"').next()).map(str::to_owned))
        .collect()
}

/// VM `name`'s block of a Vagrantfile (`config.vm.define "<name>"`).
pub(crate) fn define<'v>(vf: &'v isoloom_test_support::RubyBlock, name: &str) -> &'v isoloom_test_support::RubyBlock {
    vf.block(&format!("config.vm.define \"{name}\""))
}

/// The provisioners VM block `vm` runs, by name (`m.vm.provision "shell", name: "<name>"`), in
/// order.
pub(crate) fn provisioners(vm: &isoloom_test_support::RubyBlock) -> Vec<String> {
    vm.every_statement()
        .iter()
        .filter(|s| s.starts_with("m.vm.provision "))
        .filter_map(|s| s.split("name: \"").nth(1).and_then(|r| r.split('"').next()).map(str::to_owned))
        .collect()
}

/// The set-up script a cloud machine runs from its user data (`#!/bin/sh`, Isoloom's own), read
/// from its resources (those named `machine`) whatever attribute the cloud takes it in
/// (`user_data_base64`, `custom_data`, `metadata_startup_script`, a `metadata` block or map).
pub(crate) fn set_up(tf: &str, machine: &str) -> Outcome<String> {
    let body = isoloom_test_support::hcl_body(tf);
    let found: Vec<String> = body
        .blocks()
        .filter(|b| b.identifier() == "resource" && b.labels().get(1).is_some_and(|l| l.as_str() == machine))
        .flat_map(|b| isoloom_test_support::hcl_literals(b.body()))
        .filter(|l| l.starts_with("#!/bin/sh\n# Isoloom: this machine sets itself up"))
        .collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        _ => Err(format!("{} set-up scripts for `{machine}` in:\n{tf}", found.len()).into()),
    }
}

/// The keys of a module's `published` output (`"<machine>/<port>"`), in order.
pub(crate) fn published_keys(body: &isoloom_test_support::hcl::Body) -> Vec<String> {
    let outputs = isoloom_test_support::hcl_blocks(body, "output", &["published"]);
    match outputs.first().and_then(|o| isoloom_test_support::hcl_attribute(o.body(), "value")) {
        Some(isoloom_test_support::hcl::Expression::Object(object)) => object.keys().map(|k| k.to_string().trim_matches('"').to_owned()).collect(),
        _ => Vec::new(),
    }
}

/// The outputs every cloud's module has, shaped as AWS's are: `ip`, and `ready_file` at the
/// machines' ready file.
pub(crate) fn assert_cloud_outputs(body: &isoloom_test_support::hcl::Body) {
    assert_eq!(isoloom_test_support::hcl_blocks(body, "output", &["ip"]).len(), 1);
    let ready = isoloom_test_support::hcl_blocks(body, "output", &["ready_file"]);
    let value = ready.first().map(|o| isoloom_test_support::hcl_text(o.body(), "value"));
    assert_eq!(value.as_deref(), Some("/var/lib/isoloom/ready"));
}

/// The comment lines a generated file opens with (`# Generated by isoloom …`, `# Start: …`), each
/// without its `# `: what a format with no place for prose says in its header.
pub(crate) fn header(text: &str) -> Vec<&str> {
    text.lines()
        .take_while(|l| l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').strip_prefix(' ').unwrap_or_default())
        .collect()
}

/// The line of runner script `runner` that runs check `name` (`if …; then pass '<name>'; else
/// fail …; fi`): a check's command asserted within its own step.
pub(crate) fn check_line<'a>(runner: &'a str, name: &str) -> Outcome<&'a str> {
    let pass = format!("; then pass {}; ", isoloom_spec::shell::quote(name));
    runner
        .lines()
        .find(|l| l.starts_with("if ") && l.contains(&pass))
        .ok_or_else(|| format!("no check `{name}` in:\n{runner}").into())
}
