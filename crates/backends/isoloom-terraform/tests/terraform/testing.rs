//! What the integration tests share: specs from the repository's examples or written in a
//! test, checked valid, and the file a backend wrote at a path.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use hcl::template::{Element, Template};
use isoloom_plan::{Environment, Lab};
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::host_access::{Grants, HostAccess};
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule, ProxmoxBackend};
use isoloom_test_support::hcl_resource;

/// A Windows Server the controller provisions: an environment with a controller and a Windows password.
pub(crate) const WINDOWS_PROVISIONED: &str = "version: 1
name: t
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  dc: { networks: { lan: 10 }, os: windows-server@2022 }
provision:
  - ansible: provision/site.yml
";

/// A test's outcome: a failure is the report.
pub(crate) type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The repository's example `name` (its folder under `examples/`), checked valid.
///
/// # Errors
///
/// When it doesn't load, or isn't valid.
pub(crate) fn example(name: &str) -> Outcome<Lab> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name);
    valid(Lab::new(isoloom_spec::load(&dir)?))
}

/// The spec `yaml` (the authored format), checked valid.
///
/// # Errors
///
/// When it doesn't parse, or isn't valid.
pub(crate) fn spec(yaml: &str) -> Outcome<Lab> {
    valid(parsed(yaml)?)
}

/// The plan of `yaml`, a valid spec, for `target`.
///
/// # Errors
///
/// When the spec doesn't parse, or isn't valid.
pub(crate) fn plan(yaml: &str, target: Target) -> Outcome<Environment> {
    planned(&spec(yaml)?, target)
}

/// The plan of `lab` for `target`: its spec lowered for the target, then planned.
///
/// # Errors
///
/// When it can't be planned.
pub(crate) fn planned(lab: &Lab, target: Target) -> Outcome<Environment> {
    Ok(lab.plan(&lab.for_target(target), target, None)?)
}

/// `cloud`'s `cloud-vm` module for `yaml`, a valid spec.
///
/// # Errors
///
/// When the spec isn't valid, or the cloud refuses it.
pub(crate) fn cloud_vm(cloud: Cloud, yaml: &str) -> Outcome<String> {
    Ok(CloudVmModule::new(cloud).render(&plan(yaml, Target::CloudVm)?)?.contents)
}

/// The Proxmox module for `yaml`, a valid spec.
///
/// # Errors
///
/// When the spec isn't valid, or Proxmox refuses it.
pub(crate) fn proxmox(yaml: &str) -> Outcome<String> {
    Ok(file(&ProxmoxBackend.render(&plan(yaml, Target::Proxmox)?)?, "proxmox/main.tf")?.to_owned())
}

/// Why `backend` refuses `yaml` (a spec, valid or not), if it does.
///
/// # Errors
///
/// When the spec doesn't parse, or can't be planned.
pub(crate) fn refusal(backend: &dyn Backend, yaml: &str) -> Outcome<Option<String>> {
    Ok(backend.refusal(&planned(&parsed(yaml)?, backend.target())?))
}

/// The environment of `yaml`, valid or not, with every access to the computer that runs it
/// allowed: these tests are of what the backends write, and the consent to a spec reaching that
/// computer is the caller's (`--allow-host`), tested with the plan.
fn parsed(yaml: &str) -> Outcome<Lab> {
    Lab::parse(yaml)
        .map(|lab| lab.with_host_access(Grants::of(HostAccess::ALL)))
        .map_err(|problems| format!("the spec doesn't parse: {problems:?}").into())
}

/// `lab`, when nothing is wrong with its spec.
fn valid(lab: Lab) -> Outcome<Lab> {
    let problems = lab.problems();
    if problems.is_empty() {
        Ok(lab)
    } else {
        Err(format!("the spec isn't valid: {problems:?}").into())
    }
}

/// The contents of the file among `files` whose path ends with `suffix`.
///
/// # Errors
///
/// When there is none.
pub(crate) fn file<'a>(files: &'a [GeneratedFile], suffix: &str) -> Outcome<&'a str> {
    files
        .iter()
        .find(|f| f.path.ends_with(suffix))
        .map(|f| f.contents.as_str())
        .ok_or_else(|| format!("no file ending with {suffix}").into())
}

/// A Docker host as the `docker` target's files give it (`isoloom-targets` builds it): the
/// Compose file's folder, its start, the values its containers read.
#[must_use]
pub(crate) fn docker_host(values: &[&str]) -> isoloom_terraform::CloudDockerHost {
    isoloom_terraform::CloudDockerHost {
        outputs: vec![".isoloom/docker".to_owned()],
        start: vec!["cd /opt/isoloom && docker compose --project-name env --file .isoloom/docker/compose.yml up -d --build --wait".to_owned()],
        values: values.iter().map(|v| (*v).to_owned()).collect(),
    }
}

/// The texts (scripts, user data, files) resource `kind` `name` of the module `tf` holds, as
/// they read once Terraform has read them: each template's text with its interpolations as
/// `${...}`, and each string written inside an interpolation (a file a script writes,
/// `${base64encode("...")}`) a text of its own.
///
/// # Panics
///
/// When `tf` isn't HCL or doesn't have exactly one such resource.
pub(crate) fn texts(tf: &str, kind: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    texts_of_body(&hcl_resource(tf, kind, name), &mut out);
    out
}

fn texts_of_body(body: &hcl::Body, out: &mut Vec<String>) {
    for structure in body {
        match structure {
            hcl::Structure::Attribute(a) => texts_of(a.expr(), out),
            hcl::Structure::Block(b) => texts_of_body(b.body(), out),
        }
    }
}

fn texts_of(expr: &hcl::Expression, out: &mut Vec<String>) {
    use hcl::Expression as E;
    match expr {
        E::String(s) => out.push(s.clone()),
        E::TemplateExpr(t) => {
            let Ok(template) = Template::from_expr(t) else {
                return out.push(t.to_string());
            };
            let mut text = String::new();
            for element in template.elements() {
                match element {
                    Element::Literal(l) => text.push_str(l),
                    Element::Interpolation(i) => {
                        text.push_str("${");
                        text.push_str(&hcl::format::to_string(&i.expr).unwrap_or_default());
                        text.push('}');
                        texts_of(&i.expr, out);
                    }
                    Element::Directive(_) => text.push_str("%{...}"),
                }
            }
            out.push(text);
        }
        E::Array(items) => items.iter().for_each(|e| texts_of(e, out)),
        E::Object(object) => object.values().for_each(|e| texts_of(e, out)),
        E::Parenthesis(inner) => texts_of(inner, out),
        E::FuncCall(call) => call.args.iter().for_each(|e| texts_of(e, out)),
        E::Conditional(c) => {
            texts_of(&c.true_expr, out);
            texts_of(&c.false_expr, out);
        }
        _ => {}
    }
}

/// The first line of `texts` that holds `what`.
///
/// # Errors
///
/// When none does.
pub(crate) fn line_with<'t>(texts: &'t [String], what: &str) -> Outcome<&'t str> {
    texts
        .iter()
        .flat_map(|t| t.lines())
        .find(|l| l.contains(what))
        .ok_or_else(|| format!("no line with {what} in {texts:?}").into())
}

/// Whether a line of one of `texts` holds `what`.
pub(crate) fn has_line(texts: &[String], what: &str) -> bool {
    texts.iter().flat_map(|t| t.lines()).any(|l| l.contains(what))
}

/// The files cloud-init writes for resource `kind` `name` of `tf` (each object of a
/// `write_files` its user data encodes, with a `path`): each one's fields (`path`, `content`,
/// `permissions`), as text, in order.
///
/// # Panics
///
/// When `tf` isn't HCL or doesn't have exactly one such resource.
pub(crate) fn written_files(tf: &str, kind: &str, name: &str) -> Vec<BTreeMap<String, String>> {
    fn in_body(body: &hcl::Body, out: &mut Vec<BTreeMap<String, String>>) {
        for structure in body {
            match structure {
                hcl::Structure::Attribute(a) => files_in(a.expr(), out),
                hcl::Structure::Block(b) => in_body(b.body(), out),
            }
        }
    }
    let mut out = Vec::new();
    in_body(&hcl_resource(tf, kind, name), &mut out);
    out
}

/// The file at `path` among `files` ([`written_files`]).
///
/// # Errors
///
/// When there is none.
pub(crate) fn written<'a>(files: &'a [BTreeMap<String, String>], path: &str) -> Outcome<&'a BTreeMap<String, String>> {
    files
        .iter()
        .find(|f| f.get("path").is_some_and(|p| p == path))
        .ok_or_else(|| format!("no file {path} among {files:?}").into())
}

fn files_in(expr: &hcl::Expression, out: &mut Vec<BTreeMap<String, String>>) {
    use hcl::Expression as E;
    match expr {
        E::Object(object) => {
            let fields: BTreeMap<String, String> = object
                .iter()
                .filter_map(|(k, v)| {
                    let hcl::ObjectKey::Identifier(id) = k else { return None };
                    let mut texts = Vec::new();
                    texts_of(v, &mut texts);
                    texts.pop().map(|t| (id.to_string(), t))
                })
                .collect();
            if fields.contains_key("path") {
                out.push(fields);
            }
            object.values().for_each(|v| files_in(v, out));
        }
        E::Array(items) => items.iter().for_each(|e| files_in(e, out)),
        E::Parenthesis(inner) => files_in(inner, out),
        E::FuncCall(call) => call.args.iter().for_each(|e| files_in(e, out)),
        E::Conditional(c) => {
            files_in(&c.true_expr, out);
            files_in(&c.false_expr, out);
        }
        _ => {}
    }
}

/// The references attribute `key` of `body` lists (`depends_on`, `replace_triggered_by`), as HCL
/// writes them.
pub(crate) fn references(body: &hcl::Body, key: &str) -> Vec<String> {
    match isoloom_test_support::hcl_attribute(body, key) {
        Some(hcl::Expression::Array(items)) => items.iter().map(ToString::to_string).collect(),
        _ => Vec::new(),
    }
}
