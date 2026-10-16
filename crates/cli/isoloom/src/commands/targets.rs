//! `isoloom targets`: the targets the spec can be generated for, why the others can't, and the
//! tier of each one generated.

use isoloom_spec::Target;
use isoloom_targets::Lab;
use isoloom_targets::tiers::{self, Tier, TierEntry};
use serde_json::{Value, json};

use super::{Console, Listing, Result, Verdict, spec};

/// Lists every target, generated or not, with the reason when not; then the tiers of those
/// generated.
pub(crate) fn run(a: &Listing, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let lab = spec::checked(spec::lab(profile, &a.project.dir)?, &a.project.dir)?;
    let entries = tiers::tiers()?;
    let ready = ready(&lab);
    if a.json {
        let listed: Vec<Value> = ready
            .iter()
            .map(|t| json!({ "id": t.id(), "tiers": tiers::of(&entries, *t).iter().map(|e| e.json()).collect::<Vec<_>>() }))
            .collect();
        console.json(&listed)?;
    } else {
        for line in listing(&lab).into_iter().chain(tier_lines(&entries, &ready, &lab)) {
            console.line(line)?;
        }
    }
    Ok(Verdict::Pass)
}

/// The targets `generate` really produces: possible by the machines' forms, and not refused by
/// the generator.
fn ready(lab: &Lab) -> Vec<Target> {
    lab.possible().into_iter().filter(|t| isoloom_targets::refusal(lab, *t).is_none()).collect()
}

/// One line per target, `✓` or `✗` with the reason; under a generated `cloud-vm`, the clouds
/// whose module is dropped.
fn listing(lab: &Lab) -> Vec<String> {
    let possible = lab.possible();
    let mut lines = Vec::new();
    for t in Target::ALL {
        if !possible.contains(&t) {
            lines.push(format!("✗ {} ({})", t.id(), why_not(lab, t)));
            continue;
        }
        // A target can be possible by its machines' forms and still be refused by its generator
        // (a Windows machine on Proxmox, say). A ✓ here means `generate` really produces it; a
        // refusal shows as ✗ with the generator's reason.
        match isoloom_targets::refusal(lab, t) {
            None => {
                lines.push(format!("✓ {}", t.id()));
                // A cloud whose model doesn't fit drops its module, not the target.
                for r in isoloom_targets::cloud_refusals(lab, t).map(|c| c.refused).unwrap_or_default() {
                    lines.push(format!("    ✗ {} ({})", r.cloud, r.reason));
                }
            }
            Some(why) => lines.push(format!("✗ {} ({why})", t.id())),
        }
    }
    lines
}

/// The tiers of the targets generated, after a blank line: a target's tier and its evidence, or,
/// for a target with providers or clouds, each tier with the ones at it (a cloud whose module is
/// dropped left out). Nothing when no target is generated.
fn tier_lines(entries: &[TierEntry], ready: &[Target], lab: &Lab) -> Vec<String> {
    if ready.is_empty() {
        return Vec::new();
    }
    let width = ready.iter().map(|t| t.id().len()).max().unwrap_or_default();
    let mut lines = vec![
        String::new(),
        "Tiers (1: proven by real runs on every host OS it exists on, 2: generated and validated, 3: experimental; `isoloom matrix` has them all):".to_owned(),
    ];
    for t in ready {
        let dropped: Vec<&str> = isoloom_targets::cloud_refusals(lab, *t)
            .map(|c| c.refused.into_iter().map(|r| r.cloud).collect())
            .unwrap_or_default();
        let mine: Vec<&TierEntry> = tiers::of(entries, *t)
            .into_iter()
            .filter(|e| e.variant.is_none_or(|v| !dropped.contains(&v)))
            .collect();
        let rows: Vec<(Tier, String)> = match mine.as_slice() {
            [one] if one.variant.is_none() => vec![(one.tier(), one.evidence())],
            _ => [Tier::One, Tier::Two, Tier::Three]
                .into_iter()
                .filter_map(|tier| {
                    let at: Vec<&str> = mine.iter().filter(|e| e.tier() == tier).filter_map(|e| e.variant).collect();
                    (!at.is_empty()).then(|| (tier, at.join(", ")))
                })
                .collect(),
        };
        for (i, (tier, what)) in rows.into_iter().enumerate() {
            let name = if i == 0 { t.id() } else { "" };
            lines.push(format!("  {name:<width$}  {tier}  {what}"));
        }
    }
    lines
}

/// Why the lab can't run on a target at all.
fn why_not(lab: &Lab, t: Target) -> String {
    if lab.source().cloud.is_some() && t != Target::CloudServices {
        "cloud services run on `cloud-services` only".to_owned()
    } else if t == Target::CloudServices {
        "needs `cloud:` (cloud services instead of machines)".to_owned()
    } else {
        isoloom_targets::not_possible_reason(lab, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tiers_follow_the_listing_one_line_per_tier_of_a_target_with_providers() {
        let entries = tiers::tiers().unwrap();
        let lab = Lab::new(
            isoloom_spec::parse(
                "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { os: debian@12, networks: { lan: 10 } }\n  b: { os: debian@12, networks: { lan: 11 } }\n",
            )
            .unwrap(),
        );
        let lines = tier_lines(&entries, &[Target::Docker, Target::Vagrant, Target::CloudVm], &lab);
        assert_eq!(lines.first().map(String::as_str), Some(""));
        assert!(lines.iter().any(|l| l.starts_with("  docker    1  real runs: linux (jarvis, ")), "{lines:#?}");
        let vagrant = lines.iter().position(|l| l.starts_with("  vagrant   ")).unwrap();
        assert!(lines[vagrant].contains("  1  libvirt"), "{lines:#?}");
        assert!(lines[vagrant + 1].starts_with("            2  virtualbox, "), "{lines:#?}");
        let clouds = lines.iter().find(|l| l.starts_with("  cloud-vm  ")).unwrap();
        assert!(
            clouds.contains("aws") && !clouds.contains("digitalocean"),
            "DigitalOcean takes one machine: {clouds}"
        );
        assert_eq!(tier_lines(&entries, &[], &lab), Vec::<String>::new());
    }
}
