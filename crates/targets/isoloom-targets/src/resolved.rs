//! The resolved snapshot, `.isoloom/resolved.json`: the spec after Isoloom has worked
//! everything out, for programs. Every address (machines on each network, and on Docker's
//! blocks; the router, the controller, the gateways), routes, start order, published ports,
//! the targets (possible, generated, refused with reasons, and on `cloud-vm` the clouds
//! generated and refused), and the checks by position with their runners. The generators and
//! this file use the same functions, and a test keeps them in agreement; tools embedding
//! Isoloom read this instead of parsing a Vagrantfile.

mod checks;
mod cloud;
mod machines;
mod message;
mod networks;
pub(crate) mod ports;
mod targets;

use isoloom_plan::Lab;
use isoloom_render::GeneratedFile;
use isoloom_spec::model::{Spec, Target};
use serde_json::{Map, Value, json};

use crate::{Error, Result};

pub(crate) use message::manifest_message;
pub use message::{fill, lookup, render_message, render_message_on};

/// The format of the snapshot; bumped when a field changes meaning or goes away (3: a machine's
/// `ports` keyed by name, each with its `protocol`, `published` and `url`).
pub const RESOLVED_VERSION: u32 = 3;

/// Where the snapshot is written, relative to the project folder.
pub const PATH: &str = ".isoloom/resolved.json";

/// The snapshot as a generated file.
pub(crate) fn file(lab: &Lab) -> GeneratedFile {
    snapshot_file(&resolve(lab))
}

/// The snapshot of instance `n` of the lab.
pub(crate) fn file_with(lab: &Lab, n: u8) -> Result<GeneratedFile> {
    resolve_with(lab, Some(n)).map(|s| snapshot_file(&s))
}

/// `snapshot` as the file it is written to.
fn snapshot_file(snapshot: &Value) -> GeneratedFile {
    // `{:#}` is serde_json's pretty printer (two-space indent), and a `Value` always prints.
    GeneratedFile {
        path: PATH.to_owned(),
        contents: format!("{snapshot:#}\n"),
    }
}

/// The snapshot's entries under `key` (`networks`, `machines`, a machine's `ports`), by name:
/// what the reports and the graph read.
pub(crate) fn entries<'a>(snapshot: &'a Value, key: &str) -> impl Iterator<Item = (&'a String, &'a Value)> {
    snapshot.get(key).and_then(Value::as_object).into_iter().flatten()
}

/// The snapshot as JSON (checked labs only).
#[must_use]
pub fn resolve(lab: &Lab) -> Value {
    snapshot(lab, &lab.spec(), None)
}

/// The snapshot of an instance (`None`: the lab as written): every form, no target's own facts.
///
/// # Errors
///
/// [`Error::Instance`] when the lab can't be made instance `instance`.
pub fn resolve_with(lab: &Lab, instance: Option<u8>) -> Result<Value> {
    let Some(n) = instance else { return Ok(resolve(lab)) };
    let spec = isoloom_spec::instance::apply(&lab.spec(), n).map_err(|source| Error::Instance { number: n, source })?;
    Ok(snapshot(lab, &spec, Some(n)))
}

/// The snapshot of `spec`, the lab as instance `instance` (or as written).
fn snapshot(lab: &Lab, spec: &Spec, instance: Option<u8>) -> Value {
    let docker = isoloom_plan::environment::lay_out(spec, Target::Docker);
    let routed = isoloom_render::router::needed(spec);
    let mut out = Map::new();
    out.insert(
        "generated_by_isoloom".into(),
        json!("from isoloom.yml; don't edit: change the spec and run `isoloom generate` (`isoloom check` fails when this file is out of date)"),
    );
    out.insert("resolved_version".into(), json!(RESOLVED_VERSION));
    out.insert("name".into(), json!(spec.name));
    out.insert("instance".into(), json!(instance));
    out.insert("inputs".into(), json!(spec.inputs));
    out.insert("networks".into(), networks::networks(spec, routed).into());
    out.insert(
        "reach".into(),
        spec.reach.iter().map(|r| json!({ "from": r.from, "to": r.to, "ports": r.ports })).collect(),
    );
    let mut resolved_machines = machines::machines(spec, &docker);
    machines::add_services(&mut resolved_machines, lab);
    out.insert("machines".into(), resolved_machines.into());
    out.insert("start_order".into(), json!(isoloom_plan::addressing::start_order(spec)));
    out.insert("clones".into(), json!(spec.clones));
    out.insert("tools".into(), networks::tools(spec, &docker).into());
    out.insert(
        "groups".into(),
        spec.groups
            .keys()
            .map(|g| (g.clone(), json!(isoloom_spec::groups::members(spec, g))))
            .collect::<Map<_, _>>()
            .into(),
    );
    out.insert("router".into(), json!(networks::router(spec, routed)));
    out.insert("controller".into(), networks::controller(spec));
    out.insert("published".into(), ports::published(spec).into());
    out.insert("targets".into(), targets::section(lab));
    out.insert("checks".into(), checks::section(spec));
    if let Some(c) = &spec.cloud {
        out.insert("cloud".into(), cloud::section(c));
    }
    // The message, with its placeholders filled from everything above (a cloud-services
    // message keeps its `{{ cloud.outputs.* }}`: they are only known once deployed).
    let mut snapshot = Value::Object(out);
    if let Some(m) = &spec.message {
        let filled = fill(m, &snapshot).ok().or_else(|| spec.cloud.as_ref().map(|_| m.clone()));
        if let Value::Object(map) = &mut snapshot {
            map.insert("message".into(), filled.map_or(Value::Null, Value::String));
        }
    }
    snapshot
}
