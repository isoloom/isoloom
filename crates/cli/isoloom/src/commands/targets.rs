//! `isoloom targets`: the targets the spec can be generated for, why the others can't, and the
//! validators that check each one generated.

use isoloom_spec::Target;
use isoloom_targets::Lab;
use isoloom_targets::validators;
use serde_json::{Value, json};

use super::{Console, Listing, Result, Verdict, spec};

/// Lists every target, generated or not, with the reason when not; then the validators of those
/// generated.
pub(crate) fn run(a: &Listing, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let (lab, _) = spec::with_settings(profile, &a.project.dir, &a.settings)?;
    let lab = spec::checked(lab, &a.project.dir)?;
    let ready = ready(&lab)?;
    if a.json {
        let listed: Vec<Value> = ready
            .iter()
            .map(|t| {
                let v = validators::of(*t);
                json!({ "id": t.id(), "writes": v.writes, "validators": v.validators })
            })
            .collect();
        console.json(&listed)?;
    } else {
        for line in listing(&lab)?.into_iter().chain(validator_lines(&ready)) {
            console.line(line)?;
        }
    }
    Ok(Verdict::Pass)
}

/// The targets `generate` really produces: possible by the machines' forms, and not refused by
/// the generator. A target whose generation fails fails the listing, as it fails `generate`.
fn ready(lab: &Lab) -> Result<Vec<Target>> {
    let mut ready = Vec::new();
    for t in lab.possible() {
        if isoloom_targets::refusal(lab, t)?.is_none() {
            ready.push(t);
        }
    }
    Ok(ready)
}

/// One line per target, `✓` or `✗` with the reason; under a generated `cloud-vm`, the clouds
/// whose module is dropped.
fn listing(lab: &Lab) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for t in Target::ALL {
        if let Some(why) = lab.not_possible(t) {
            lines.push(format!("✗ {} ({why})", t.id()));
            continue;
        }
        // A target can be possible by its machines' forms and still be refused by its generator
        // (a Windows machine on Proxmox, say). A ✓ here means `generate` really produces it; a
        // refusal shows as ✗ with the generator's reason.
        match isoloom_targets::refusal(lab, t)? {
            None => {
                lines.push(format!("✓ {}", t.id()));
                // A cloud whose model doesn't fit drops its module, not the target.
                for r in isoloom_targets::cloud_refusals(lab, t)?.map(|c| c.refused).unwrap_or_default() {
                    lines.push(format!("    ✗ {} ({})", r.cloud, r.reason));
                }
            }
            Some(why) => lines.push(format!("✗ {} ({why})", t.id())),
        }
    }
    Ok(lines)
}

/// The validators of the targets generated, after a blank line: each target's, on one line.
/// Nothing when no target is generated.
fn validator_lines(ready: &[Target]) -> Vec<String> {
    if ready.is_empty() {
        return Vec::new();
    }
    let width = ready.iter().map(|t| t.id().len()).max().unwrap_or_default();
    let mut lines = vec![
        String::new(),
        "Validated by (the official validators of each target's files; `isoloom matrix` has them all):".to_owned(),
    ];
    for t in ready {
        lines.push(format!("  {:<width$}  {}", t.id(), validators::of(*t).validators.join(", ")));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_validators_follow_the_listing_one_line_per_target() {
        let lines = validator_lines(&[Target::Docker, Target::Vagrant, Target::CloudVm]);
        assert_eq!(lines.first().map(String::as_str), Some(""));
        assert!(lines.iter().any(|l| l == "  docker    docker compose config"), "{lines:#?}");
        assert!(lines.iter().any(|l| l == "  vagrant   vagrant validate"), "{lines:#?}");
        assert!(
            lines.iter().any(|l| l == "  cloud-vm  terraform validate, tflint, terraform test"),
            "{lines:#?}"
        );
        assert_eq!(validator_lines(&[]), Vec::<String>::new());
    }
}
