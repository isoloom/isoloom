//! Helpers shared by the Terraform run-manifest builders (the clouds and the Proxmox server):
//! the small template and step-id utilities, a bare machine, a target machine with its ports
//! fixed, and a state-read output. Each builder differs in its lifecycle, credentials and the
//! machines it makes; these are what they have in common, kept in one place so no copy drifts.

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::num::NonZeroU32;

use isoloom_manifest::{
    Delivery, EnvName, Error as ManifestError, Identifier, Machine, Name, NonEmpty, OsName, Output, OutputRole, OutputSource, Param, Part, Placeholder,
    PublishMode, Role, RunVar, SetBy, SnakeName, StepId, Template, TfVarShape,
};
use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::run_manifest::{addresses, arch, port};

use crate::machine::vms;
use crate::module::variable::Variable;

/// A step id.
pub(crate) fn id(text: &str) -> Result<StepId, ManifestError> {
    StepId::parse(text)
}

/// A timeout as a number of seconds, `None` when zero (no timeout).
pub(crate) fn secs(n: u32) -> Option<NonZeroU32> {
    NonZeroU32::new(n)
}

/// `${run.<var>}` within text.
pub(crate) fn run_var(before: &str, var: RunVar, after: &str) -> Template {
    Template::from_parts([Part::Text(before.into()), Part::Placeholder(Placeholder::Run(var)), Part::Text(after.into())])
}

/// `${outputs.<name>}`.
pub(crate) fn output(name: &str) -> Result<Template, ManifestError> {
    Ok(Template::from(Placeholder::Output(SnakeName::parse(name)?)))
}

/// `words` as templates, each plain text.
pub(crate) fn words(words: &[&str]) -> Vec<Template> {
    words.iter().map(|w| Template::text(w)).collect()
}

/// A `terraform` step's environment: its data folder in the run's state, automation on and no
/// prompt, then `extra` (the run-time values the runner gives the module, such as its key).
pub(crate) fn terraform_env(extra: Vec<(&str, Template)>) -> Result<BTreeMap<EnvName, Template>, ManifestError> {
    let mut all = vec![
        ("TF_DATA_DIR", run_var("", RunVar::StateDir, "/.terraform")),
        ("TF_IN_AUTOMATION", Template::text("1")),
        ("TF_INPUT", Template::text("0")),
    ];
    all.extend(extra);
    all.into_iter().map(|(k, v)| Ok((EnvName::parse(k)?, v))).collect()
}

/// A bare machine on its `addresses`: no OS, resources, ports or volumes set.
pub(crate) fn machine(name: &str, role: Role, addresses: BTreeMap<Name, IpAddr>) -> Result<Machine, ManifestError> {
    Ok(Machine {
        name: Name::parse(name)?,
        role,
        of: None,
        os: None,
        windows: false,
        arch: None,
        resources: None,
        addresses,
        keep_running: false,
        native: None,
        volumes: Vec::new(),
        ports: Vec::new(),
    })
}

/// A target machine: a bare machine with its OS, Windows flag and architecture, then each spec
/// port fixed at its own number on `at` (the machine's public address).
pub(crate) fn target_machine(name: &str, mp: &MachinePlan, spec: &isoloom_spec::model::Machine, at: &Template) -> Result<Machine, ManifestError> {
    let mut m = machine(name, Role::Target, addresses(mp)?)?;
    m.os = mp.os.as_deref().map(OsName::parse).transpose()?;
    m.windows = mp.is_windows();
    m.arch = Some(arch(mp.arch));
    for p in &spec.ports {
        let mut port = port(&spec.port_label(p), p, &mp.ports, at)?;
        if let Some(publish) = port.publish.as_mut() {
            publish.mode = PublishMode::Fixed;
        }
        m.ports.push(port);
    }
    Ok(m)
}

/// The Terraform variable `name`, a string.
pub(crate) fn tf_var_string(name: &str) -> Result<NonEmpty<Delivery>, ManifestError> {
    Ok(NonEmpty::one(Delivery::TfVar {
        var: SnakeName::parse(name)?,
        shape: TfVarShape::String,
    }))
}

/// Variable `v` as a param the embedder gives: required when it has no default (or `required`), a
/// `bool` taking `true` or `false`.
pub(crate) fn param(v: &Variable, required: bool) -> Result<Param, ManifestError> {
    Ok(Param {
        name: EnvName::parse(v.name())?,
        description: v.description().map_or_else(|| format!("The module's `{}`.", v.name()), |d| format!("{d}.")),
        default: v.default_text(),
        values: if v.is_bool() { vec!["true".into(), "false".into()] } else { Vec::new() },
        required: required || !v.has_default(),
        hosts: BTreeSet::new(),
        providers: BTreeSet::new(),
        set_by: SetBy::Embedder,
        deliver: tf_var_string(v.name())?,
    })
}

/// The seal's run-manifest param (`crate::cloud::vm::seal`): the runner sets it `true` once the
/// machines are set up (its default is `false`), delivered to the module's `isoloom_sealed`
/// Terraform variable. `closed_at` names where the internet is closed for the description (the
/// clouds' "network", a server's "router").
pub(crate) fn seal_param(name: &str, closed_at: &str) -> Result<Param, ManifestError> {
    Ok(Param {
        name: EnvName::parse(name)?,
        description: format!("true once the machines are set up: the internet is closed to the `internet: false` networks, at the {closed_at}."),
        default: Some("false".into()),
        values: vec!["true".into(), "false".into()],
        required: false,
        hosts: BTreeSet::new(),
        providers: BTreeSet::new(),
        set_by: SetBy::Runner,
        deliver: tf_var_string(name)?,
    })
}

/// The VMs that read the launch-time input `name`, in start order (empty: none do).
pub(crate) fn input_readers(env: &Environment, name: &str) -> Result<Vec<Name>, ManifestError> {
    vms(env)
        .filter(|(_, mp)| mp.inputs.iter().any(|i| i == name))
        .map(|(n, _)| Name::parse(n))
        .collect()
}

/// A Terraform output read from the module's state (`sensitive: false`, no format).
pub(crate) fn state_output(name: &str, role: OutputRole, what: &str) -> Result<Output, ManifestError> {
    Ok(Output {
        name: SnakeName::parse(name)?,
        description: Some(what.into()),
        role,
        sensitive: false,
        format: None,
        from: OutputSource::TerraformOutput {
            name: Identifier::parse(name)?,
            default: None,
        },
    })
}
