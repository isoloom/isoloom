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

pub(crate) use message::manifest_message;
pub use message::{HostPort, fill, lookup, on_target, render_message, render_message_at, render_message_on, with_published};

/// The format of the snapshot; bumped when a field changes meaning or goes away (3: a machine's
/// `ports` keyed by name, each with its `protocol`, `published` and `url`).
pub const RESOLVED_VERSION: u32 = 3;

/// Where the snapshot is written, relative to the project folder.
pub const PATH: &str = ".isoloom/resolved.json";

/// The snapshot as a generated file.
pub(crate) fn file(lab: &Lab) -> GeneratedFile {
    file_with(lab, None)
}

/// The snapshot of instance `instance` of the lab.
pub(crate) fn file_with(lab: &Lab, instance: Option<u8>) -> GeneratedFile {
    // `{:#}` is serde_json's pretty printer (two-space indent), and a `Value` always prints.
    GeneratedFile {
        path: PATH.to_owned(),
        contents: format!("{:#}\n", resolve_with(lab, instance)),
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
    resolve_with(lab, None)
}

/// The lab as the snapshot reads it: every form, no target's own facts, as instance `instance`
/// when one is asked (as written when it can't be made: generating it says why).
fn lab_spec(lab: &Lab, instance: Option<u8>) -> Spec {
    let spec = lab.spec();
    match instance {
        Some(n) => isoloom_spec::instance::apply(&spec, n).unwrap_or(spec),
        None => spec,
    }
}

/// The snapshot of an instance (`None`: the lab as written).
#[must_use]
pub fn resolve_with(lab: &Lab, instance: Option<u8>) -> Value {
    let spec = &lab_spec(lab, instance);
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
    out.insert("machines".into(), machines::machines(spec, &docker).into());
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
