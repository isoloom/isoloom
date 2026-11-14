//! The tier list against what the code says of the targets: each Vagrant provider exists on the
//! host OSes the `vagrant` run manifest gives it, and every entry names the providers and clouds each target really has.

use std::collections::BTreeSet;

use isoloom_spec::model::Target;
use isoloom_targets::generate;
use isoloom_targets::tiers;
use serde_json::Value;

use crate::testing::{Outcome, example, file};

/// The host OSes of an entry, by name.
fn exists_on(entries: &[tiers::TierEntry], id: &str) -> Outcome<BTreeSet<String>> {
    let entry = entries.iter().find(|e| e.id() == id).ok_or_else(|| format!("no tier entry `{id}`"))?;
    // A host OS by the name the tier list and the run manifest write it with.
    let named = |o| serde_json::to_value(o).ok().and_then(|v| v.as_str().map(str::to_owned));
    entry
        .exists_on
        .iter()
        .map(|o| named(o).ok_or_else(|| "a host OS without a name".into()))
        .collect()
}

#[test]
fn a_vagrant_provider_exists_where_the_run_manifest_says() -> Outcome {
    let entries = tiers::tiers()?;
    let files = generate(&example("hello-stack")?, Target::Vagrant)?;
    let manifest: Value = serde_json::from_str(file(&files, "vagrant/run.json")?)?;
    let providers = manifest["identity"]["vagrant"]["providers"].as_array().ok_or("no providers")?;
    assert_eq!(providers.len(), tiers::variants(Target::Vagrant).len());
    for p in providers {
        let id = p["id"].as_str().ok_or("a provider with no id")?;
        let hosts: BTreeSet<String> = p["hosts"]
            .as_array()
            .ok_or("no hosts")?
            .iter()
            .filter_map(|h| h.as_str().map(str::to_owned))
            .collect();
        assert_eq!(exists_on(&entries, &format!("vagrant/{id}"))?, hosts, "vagrant/{id}");
    }
    Ok(())
}
