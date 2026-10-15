//! `isoloom targets`: the targets the spec can be generated for, and why the others can't.

use std::path::PathBuf;
use std::process::ExitCode;

use isoloom_spec::Target;
use isoloom_targets::Lab;

use crate::{Res, output, spec};

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    dir: PathBuf,
    #[arg(long)]
    json: bool,
}

pub fn run(a: &Args, profile: &spec::Profile) -> Res<ExitCode> {
    let lab = spec::load_lab(profile, &a.dir)?;
    if spec::report_invalid(&a.dir, &lab) {
        return Ok(ExitCode::FAILURE);
    }
    if a.json {
        let ids: Vec<&str> = ready(&lab).iter().map(|t| t.id()).collect();
        output::print_json(&ids)?;
    } else {
        print_text(&lab);
    }
    Ok(ExitCode::SUCCESS)
}

/// The targets `generate` really produces: possible by the machines' forms, and not refused by
/// the generator.
fn ready(lab: &Lab) -> Vec<Target> {
    lab.possible().into_iter().filter(|t| isoloom_targets::refusal(lab, *t).is_none()).collect()
}

fn print_text(lab: &Lab) {
    let possible = lab.possible();
    for t in Target::ALL {
        if !possible.contains(&t) {
            println!("✗ {} ({})", t.id(), why_not(lab, t));
            continue;
        }
        // A target can be possible by its machines' forms and still be refused by its generator
        // (a Windows machine on Proxmox, say). A ✓ here means `generate` really produces it; a
        // refusal shows as ✗ with the generator's reason.
        match isoloom_targets::refusal(lab, t) {
            None => {
                println!("✓ {}", t.id());
                // A cloud whose model doesn't fit drops its module, not the target.
                for (cloud, why) in isoloom_targets::cloud_refusals(lab, t).map(|(_, r)| r).unwrap_or_default() {
                    println!("    ✗ {cloud} ({why})");
                }
            }
            Some(why) => println!("✗ {} ({why})", t.id()),
        }
    }
}

/// Why the lab can't run on a target at all.
fn why_not(lab: &Lab, t: Target) -> String {
    if lab.source().cloud.is_some() && t != Target::CloudServices {
        "cloud services run on `cloud-services` only".to_string()
    } else if t == Target::CloudServices {
        "needs `cloud:` (cloud services instead of machines)".to_string()
    } else {
        isoloom_targets::resolved::not_possible_reason(lab, t)
    }
}
