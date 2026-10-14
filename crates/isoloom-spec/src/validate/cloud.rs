//! `cloud`: cloud services, a Terraform module and what it exposes, with no machines.

use super::Problems;
use crate::model::{CloudServices, Spec};

/// What `cloud.env` can't set: the cloud-services scripts' own variables, and the shell's.
/// `ISOLOOM_*` is refused too (the scripts export `ISOLOOM_PROJECT`).
const CLOUD_ENV_RESERVED: &[&str] = &[
    "here",
    "src",
    "module",
    "tries",
    "rc",
    "PATH",
    "IFS",
    "ENV",
    "CDPATH",
    "PS4",
    "HOME",
    "TF_DATA_DIR",
    "TF_VAR_isoloom_instance",
];

/// The most applies `cloud.apply_retries` adds after a failed one.
const MAX_APPLY_RETRIES: u8 = 5;

/// A Terraform variable's or output's name.
fn ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') && !s.starts_with(|ch: char| ch.is_ascii_digit())
}

/// An environment variable's name: letters, digits and `_`, not starting with a digit.
fn env_name(s: &str) -> bool {
    s.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_') && s.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// The cloud services: alone in the environment, a module, well-named variables and outputs,
/// inputs that are declared, a sane hourly cost, and the scripts' environment, retries and
/// host requirements.
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
        for i in CloudServices::inputs_in(v) {
            if !spec.inputs.contains(&i) {
                p.add(format!("cloud.vars.{k}"), format!("`{i}` isn't declared in the spec's `inputs`"));
            }
        }
    }
    if c.hourly_usd.is_some_and(|h| !(0.0..=1000.0).contains(&h)) {
        p.add("cloud.hourly_usd", "a cost per hour in US dollars (0 to 1000)");
    }
    scripts(c, p);
}

/// What the scripts take from the spec: `env`'s names, `apply_retries`, `requires`.
fn scripts(c: &CloudServices, p: &mut Problems) {
    for k in c.env.keys() {
        if !env_name(k) {
            p.add(
                format!("cloud.env.{k}"),
                format!("`{k}` isn't an environment variable name (letters, digits, `_`)"),
            );
        } else if CLOUD_ENV_RESERVED.contains(&k.as_str()) || k.starts_with("ISOLOOM_") {
            p.add(
                format!("cloud.env.{k}"),
                format!("`{k}` is the scripts' own (or the shell's): pick another name"),
            );
        }
    }
    if c.apply_retries > MAX_APPLY_RETRIES {
        p.add("cloud.apply_retries", format!("at most {MAX_APPLY_RETRIES} more applies"));
    }
    for (i, r) in c.requires.iter().enumerate() {
        if r.trim().is_empty() {
            p.add(format!("cloud.requires[{i}]"), "a command name, or a shell command that must succeed");
        }
    }
}
