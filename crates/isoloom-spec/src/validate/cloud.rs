//! `cloud`: cloud services, a Terraform module and what it exposes, with no machines.

use super::Problems;
use crate::model::{CloudServices, Spec};

/// A Terraform variable's or output's name.
fn ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') && !s.starts_with(|ch: char| ch.is_ascii_digit())
}

/// The cloud services: alone in the environment, a module, well-named variables and outputs,
/// inputs that are declared and a sane hourly cost.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    let Some(c) = &spec.cloud else { return };
    if !spec.machines.is_empty() {
        p.add("cloud", "an environment is cloud services or machines, not both (yet): leave `machines` out");
    }
    if c.terraform.trim().is_empty() {
        p.add("cloud.terraform", "the Terraform module's folder, in the project");
    }
    for k in c.vars.keys() {
        if !ident(k) {
            p.add(format!("cloud.vars.{k}"), format!("`{k}` isn't a Terraform variable name"));
        }
    }
    for (k, o) in &c.outputs {
        if !ident(k) || !ident(o) {
            p.add(format!("cloud.outputs.{k}"), format!("`{k}: {o}`: a name, then the module output it reads"));
        }
    }
    for (k, v) in &c.vars {
        if let Some(i) = CloudServices::input_of(v)
            && !spec.inputs.iter().any(|d| d == i)
        {
            p.add(format!("cloud.vars.{k}"), format!("`{i}` isn't declared in the spec's `inputs`"));
        }
    }
    if c.hourly_usd.is_some_and(|h| !(0.0..=1000.0).contains(&h)) {
        p.add("cloud.hourly_usd", "a cost per hour in US dollars (0 to 1000)");
    }
}
