//! The snapshot's targets: possible by the machines' forms, narrowed by `targets:`, then what
//! the generators actually produce and why they refuse the rest, with the clouds of `cloud-vm`. A
//! target whose generation fails (the environment doesn't plan for it) is neither: it is listed under
//! `failed`, a key the section has only then.

use isoloom_plan::Lab;
use isoloom_spec::model::Target;
use serde_json::{Map, Value, json};

use crate::refusal::{cloud_refusals, refusal};

/// The targets section.
pub(super) fn section(lab: &Lab) -> Value {
    let possible = lab.possible();
    let mut generated = Vec::new();
    let mut refused = Map::new();
    let mut not_possible = Map::new();
    let mut failed = Map::new();
    for t in Target::ALL {
        if let Some(why) = lab.not_possible(t) {
            not_possible.insert(t.id().into(), json!(why.to_string()));
            continue;
        }
        match refusal(lab, t) {
            Ok(Some(why)) => {
                refused.insert(t.id().into(), json!(why));
            }
            Ok(None) => generated.push(t.id()),
            Err(e) => {
                failed.insert(t.id().into(), json!(e.to_string()));
            }
        }
    }
    let mut section = json!({
        "possible": possible.iter().map(|t| t.id()).collect::<Vec<_>>(),
        "generated": generated,
        "refused": refused,
        "not_possible": not_possible,
        "clouds": clouds(lab),
    });
    if let (Value::Object(map), false) = (&mut section, failed.is_empty()) {
        map.insert("failed".into(), failed.into());
    }
    section
}

/// Per generated multi-cloud target (`cloud-vm`, `cloud-docker`), the clouds whose module is written and each
/// refused cloud with its reason: a cloud's refusal drops its module, not the target.
fn clouds(lab: &Lab) -> Map<String, Value> {
    Target::ALL
        .into_iter()
        .filter_map(|t| {
            // A target that fails is listed under `failed`, by `section`.
            let clouds = cloud_refusals(lab, t).ok()??;
            let refused: Map<String, Value> = clouds.refused.into_iter().map(|r| (r.cloud.to_owned(), json!(r.reason))).collect();
            Some((t.id().to_owned(), json!({ "generated": clouds.generated, "refused": refused })))
        })
        .collect()
}
