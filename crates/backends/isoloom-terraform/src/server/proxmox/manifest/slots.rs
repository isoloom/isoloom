//! What a Proxmox run is given at run time: the module's variables as params (the API, the node,
//! the datastores, the uplink bridge, each Windows OS's template), its sensitive ones (the
//! password, the API token) as connection slots, the lab's inputs as entries of the module's
//! `inputs` map, and the check runners' switch. The runner sets three variables itself, through
//! the steps' environment ([`RUNNER_SET`]).

use std::collections::BTreeSet;

use isoloom_manifest::{
    CheckRunner, Delivery, EnvName, Error as ManifestError, Identifier, Name, NonEmpty, Param, SecretSlot, SetBy, SlotKind, SnakeName, StepId, StepSelection,
    TfVarShape,
};
use isoloom_plan::Environment;

use crate::machine::vms;
use crate::module::variable::Variable;

/// The module's variables the runner sets itself (`steps`): never params.
pub(super) const RUNNER_SET: [&str; 3] = ["slot", "ssh_public_key", "proxmox_ssh_private_key_file"];

/// The module's variable the inputs go in, one entry each.
const INPUTS: &str = "inputs";

/// `0`: the checks derived from the spec are skipped.
const DERIVED: &str = "ISOLOOM_DERIVED";

/// The Terraform variable `name`.
fn tf_var(name: &str, shape: TfVarShape) -> Result<NonEmpty<Delivery>, ManifestError> {
    Ok(NonEmpty::one(Delivery::TfVar {
        var: SnakeName::parse(name)?,
        shape,
    }))
}

/// The module's variables an embedder sets, then the checks' switch when the lab has checks.
pub(super) fn params(env: &Environment, runners: &[CheckRunner]) -> Result<Vec<Param>, ManifestError> {
    let variables = super::super::base::variables(env);
    let mut out: Vec<Param> = variables
        .iter()
        .filter(|v| !v.is_sensitive() && v.name() != INPUTS && !RUNNER_SET.contains(&v.name()))
        .map(param)
        .collect::<Result<_, _>>()?;
    let steps: Vec<StepId> = runners.iter().map(|r| r.step.id.clone()).collect();
    if let Ok(steps) = NonEmpty::new(steps, "the check steps") {
        out.push(Param {
            name: EnvName::parse(DERIVED)?,
            description: "0: the checks derived from the spec (reachability, no internet) are skipped.".into(),
            default: Some("1".into()),
            values: vec!["0".into(), "1".into()],
            required: false,
            hosts: BTreeSet::new(),
            providers: BTreeSet::new(),
            set_by: SetBy::Embedder,
            deliver: NonEmpty::one(Delivery::Env {
                var: EnvName::parse(DERIVED)?,
                steps: StepSelection::Only(steps),
            }),
        });
    }
    Ok(out)
}

/// Variable `v` as a param the embedder sets: required when it has no default; a `bool` takes
/// `true` or `false`.
fn param(v: &Variable) -> Result<Param, ManifestError> {
    Ok(Param {
        name: EnvName::parse(v.name())?,
        description: v.description().map_or_else(|| format!("The module's `{}`.", v.name()), |d| format!("{d}.")),
        default: v.default_text(),
        values: if v.is_bool() { vec!["true".into(), "false".into()] } else { Vec::new() },
        required: !v.has_default(),
        hosts: BTreeSet::new(),
        providers: BTreeSet::new(),
        set_by: SetBy::Embedder,
        deliver: tf_var(v.name(), TfVarShape::String)?,
    })
}

/// The connection's secrets (the module's sensitive variables), then each input a machine reads.
pub(super) fn secrets(env: &Environment) -> Result<Vec<SecretSlot>, ManifestError> {
    let mut out = Vec::new();
    for v in super::super::base::variables(env).iter().filter(|v| v.is_sensitive() && v.name() != INPUTS) {
        out.push(slot(v.name(), SlotKind::Connection, true, Vec::new(), tf_var(v.name(), TfVarShape::String)?)?);
    }
    for input in &env.spec().inputs {
        let readers: Vec<Name> = vms(env)
            .filter(|(_, mp)| mp.inputs.contains(input))
            .map(|(n, _)| Name::parse(n))
            .collect::<Result<_, _>>()?;
        if readers.is_empty() {
            continue;
        }
        let sensitive = !env.spec().plain_inputs.contains(input);
        let mut s = slot(input, SlotKind::Input, sensitive, readers, tf_var(INPUTS, TfVarShape::MapEntry)?)?;
        // Left out, an input is an empty value in the map: the machines read it empty.
        s.default = Some(String::new());
        out.push(s);
    }
    Ok(out)
}

fn slot(name: &str, kind: SlotKind, sensitive: bool, machines: Vec<Name>, deliver: NonEmpty<Delivery>) -> Result<SecretSlot, ManifestError> {
    Ok(SecretSlot {
        name: Identifier::parse(name)?,
        kind,
        required: false,
        sensitive,
        machines,
        providers: BTreeSet::new(),
        source: None,
        default: None,
        deliver,
    })
}
