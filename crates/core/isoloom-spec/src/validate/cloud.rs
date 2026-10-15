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
                p.unknown(format!("cloud.vars.{k}"), format!("`{i}` isn't declared in the spec's `inputs`"));
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

#[cfg(test)]
mod tests {
    use crate::testing::problems;

    /// The problems of a cloud-services environment written as `cloud`, its input `PLAYER_IP`
    /// declared.
    fn of(cloud: &str) -> Vec<String> {
        problems(&format!("version: 1\nname: t\ninputs: [PLAYER_IP]\ncloud: {cloud}\n"))
    }

    /// The problems of `cloud.<field>` added to a sound module.
    fn with(field: &str) -> Vec<String> {
        of(&format!("{{ provider: aws, terraform: infra, {field} }}"))
    }

    #[test]
    fn sound_cloud_services_pass() {
        let ok = with(
            "vars: { region: eu, ip-list_2: ['{{ inputs.PLAYER_IP }}/32'] }, outputs: { site: site_url, api-2_x: api }, hourly_usd: 0, env: { REGION: x, _TOKEN: y, A_1: z }, apply_retries: 5, requires: [jq]",
        );
        assert_eq!(ok, Vec::<String>::new());
        assert_eq!(with("hourly_usd: 1000"), Vec::<String>::new());
    }

    #[test]
    fn cloud_services_have_no_machines_and_a_module() {
        let both = problems(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.0.0.0/24 } }\nmachines: { web: { networks: { lan: 10 }, docker: { image: nginx } } }\ncloud: { provider: aws, terraform: infra }\n",
        );
        assert_eq!(
            both,
            ["cloud: an environment is cloud services or machines, not both (yet): leave `machines` out"]
        );
        assert_eq!(
            of("{ provider: aws, terraform: ' ' }"),
            ["cloud.terraform: the Terraform module's folder, in the project"]
        );
    }

    #[test]
    fn variables_and_outputs_are_terraform_names() {
        for bad in ["1region", "re.gion", "re gion"] {
            assert_eq!(
                with(&format!("vars: {{ '{bad}': x }}")),
                [format!("cloud.vars.{bad}: `{bad}` isn't a Terraform variable name")],
                "{bad}"
            );
        }
        assert_eq!(with("vars: { '': x }"), ["cloud.vars.: `` isn't a Terraform variable name"]);
        assert_eq!(
            with("outputs: { 1site: site_url }"),
            ["cloud.outputs.1site: `1site: site_url`: a name, then the module output it reads"]
        );
        assert_eq!(
            with("outputs: { site: site.url }"),
            ["cloud.outputs.site: `site: site.url`: a name, then the module output it reads"]
        );
    }

    #[test]
    fn a_variables_inputs_are_declared() {
        assert_eq!(
            with("vars: { ip: '{{ inputs.NOPE }}' }"),
            ["cloud.vars.ip: `NOPE` isn't declared in the spec's `inputs`"]
        );
    }

    #[test]
    fn the_hourly_cost_is_0_to_1000_dollars() {
        for bad in ["-1", "1000.5"] {
            assert_eq!(
                with(&format!("hourly_usd: {bad}")),
                ["cloud.hourly_usd: a cost per hour in US dollars (0 to 1000)"],
                "{bad}"
            );
        }
    }

    #[test]
    fn the_scripts_environment_is_well_named_and_their_own_is_left_alone() {
        for bad in ["1A", "A-B"] {
            let why = format!("cloud.env.{bad}: `{bad}` isn't an environment variable name (letters, digits, `_`)");
            assert_eq!(with(&format!("env: {{ {bad}: x }}")), [why], "{bad}");
        }
        for taken in ["PATH", "tries", "ISOLOOM_PROJECT"] {
            let why = format!("cloud.env.{taken}: `{taken}` is the scripts' own (or the shell's): pick another name");
            assert_eq!(with(&format!("env: {{ {taken}: x }}")), [why], "{taken}");
        }
    }

    #[test]
    fn retries_and_requirements_are_bounded_and_named() {
        assert_eq!(with("apply_retries: 6"), ["cloud.apply_retries: at most 5 more applies"]);
        assert_eq!(
            with("requires: [jq, ' ']"),
            ["cloud.requires[1]: a command name, or a shell command that must succeed"]
        );
    }
}
