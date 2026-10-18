//! The snapshot's targets: possible by the machines' forms, narrowed by `targets:`, then what
//! the generators actually produce and why they refuse the rest, with the clouds of `cloud-vm`.

use isoloom_plan::Lab;
use isoloom_spec::model::Target;
use serde_json::{Map, Value, json};

use crate::refusal::{cloud_refusals, not_possible_reason, refusal};
use crate::registry::backend;

/// The targets section.
pub(super) fn section(lab: &Lab) -> Value {
    let possible = lab.possible();
    let mut generated = Vec::new();
    let mut refused = Map::new();
    let mut not_possible = Map::new();
    for t in Target::ALL {
        if !possible.contains(&t) {
            not_possible.insert(t.id().into(), json!(not_possible_reason(lab, t)));
        } else if backend(t).is_none() {
            refused.insert(t.id().into(), json!("no generator yet"));
        } else if let Some(why) = refusal(lab, t) {
            refused.insert(t.id().into(), json!(why));
        } else {
            generated.push(t.id());
        }
    }
    json!({
        "possible": possible.iter().map(|t| t.id()).collect::<Vec<_>>(),
        "generated": generated,
        "refused": refused,
        "not_possible": not_possible,
        "clouds": clouds(lab),
    })
}

/// Per generated multi-cloud target (`cloud-vm`), the clouds whose module is written and each
/// refused cloud with its reason: a cloud's refusal drops its module, not the target.
fn clouds(lab: &Lab) -> Map<String, Value> {
    Target::ALL
        .into_iter()
        .filter_map(|t| {
            let clouds = cloud_refusals(lab, t)?;
            let refused: Map<String, Value> = clouds.refused.into_iter().map(|r| (r.cloud.to_owned(), json!(r.reason))).collect();
            Some((t.id().to_owned(), json!({ "generated": clouds.generated, "refused": refused })))
        })
        .collect()
}
