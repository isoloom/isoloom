//! The snapshot's cloud services: what a tool reading it needs to run them (the cloud, the
//! module, what they expose and cost). Not the variables, which may take a user's input.

use isoloom_spec::model::CloudServices;
use serde_json::{Value, json};

/// The cloud section.
pub(super) fn section(c: &CloudServices) -> Value {
    // Every input the variables take, at any depth, once each.
    let mut inputs: Vec<String> = Vec::new();
    for n in c.vars.values().flat_map(CloudServices::inputs_in) {
        if !inputs.contains(&n) {
            inputs.push(n);
        }
    }
    json!({
        "provider": c.provider.id(),
        "terraform": c.terraform,
        "outputs": c.outputs.keys().collect::<Vec<_>>(),
        "inputs": inputs,
        "hourly_usd": c.hourly_usd,
    })
}
