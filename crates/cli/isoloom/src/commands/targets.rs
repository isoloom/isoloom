//! `isoloom targets`: the targets the spec can be generated for, and why the others can't.

use isoloom_spec::Target;
use isoloom_targets::Lab;

use super::{Console, Listing, Result, Verdict, spec};

/// Lists every target, generated or not, with the reason when not.
pub(crate) fn run(a: &Listing, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let lab = spec::checked(spec::lab(profile, &a.project.dir)?, &a.project.dir)?;
    if a.json {
        let ids: Vec<&str> = ready(&lab).iter().map(|t| t.id()).collect();
        console.json(&ids)?;
    } else {
        for line in listing(&lab) {
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
