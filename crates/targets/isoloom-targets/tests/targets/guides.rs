//! The target guides against the targets: a guide never says a runner delivers what its target
//! refuses.

use isoloom_plan::Feature;
use isoloom_spec::Target;

use crate::testing::Outcome;

/// The guide of the targets in `docs/guide/targets/<page>.md`.
fn guide(page: &str) -> Outcome<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/guide/targets")
        .join(format!("{page}.md"));
    Ok(std::fs::read_to_string(path)?)
}

/// A guide's paragraphs (blank-line separated), each on one line.
fn paragraphs(text: &str) -> Vec<String> {
    text.split("\n\n").map(|p| p.split_whitespace().collect::<Vec<_>>().join(" ")).collect()
}

#[test]
fn a_guide_never_has_the_runner_deliver_secrets_its_target_refuses() -> Outcome {
    for (target, page) in [(Target::Proxmox, "proxmox"), (Target::Esxi, "esxi")] {
        let backend = isoloom_targets::backend(target).ok_or("no backend")?;
        if backend.capabilities().has(Feature::Secrets) {
            continue;
        }
        for p in paragraphs(&guide(page)?) {
            let claims = p.contains("runner") && p.contains("delivers") && p.contains("secrets");
            assert!(!claims || p.contains("refuse"), "{page}.md: {p}");
        }
    }
    Ok(())
}
