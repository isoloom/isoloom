//! What a Proxmox run is given at run time: the module's variables as params (the API, the node,
//! the datastores, the uplink bridge, each Windows OS's template), its sensitive ones (the
//! password, the API token) as connection slots, the lab's inputs as entries of the module's
//! `inputs` map, and the check runners' switch. The runner sets three variables itself, through
//! the steps' environment ([`RUNNER_SET`]).

use std::collections::BTreeSet;

use isoloom_manifest::{CheckRunner, Delivery, Error as ManifestError, Identifier, Name, NonEmpty, Param, SecretSlot, SlotKind, SnakeName, TfVarShape};
use isoloom_plan::Environment;

use isoloom_render::run_manifest::derived_switch;

use crate::module::variable::Variable;

/// The run's seal switch, set by the runner once the machines are ready (`crate::server::router`).
pub(super) const SEALED: &str = crate::cloud::vm::seal::SEALED;

/// The module's variables the runner sets itself (`steps`): never params an embedder gives.
pub(super) const RUNNER_SET: [&str; 4] = ["slot", "ssh_public_key", "proxmox_ssh_private_key_file", SEALED];

/// The module's variable the inputs go in, one entry each.
const INPUTS: &str = "inputs";

/// The Terraform variable `name`.
fn tf_var(name: &str, shape: TfVarShape) -> Result<NonEmpty<Delivery>, ManifestError> {
    Ok(NonEmpty::one(Delivery::TfVar {
        var: SnakeName::parse(name)?,
        shape,
    }))
}

/// The module's variables an embedder sets (of `variables`), then the checks' switch when the lab
/// has checks.
pub(super) fn params(variables: &[Variable], runners: &[CheckRunner]) -> Result<Vec<Param>, ManifestError> {
    let mut out: Vec<Param> = variables
        .iter()
        .filter(|v| !v.is_sensitive() && v.name() != INPUTS && !RUNNER_SET.contains(&v.name()))
        .map(param)
        .collect::<Result<_, _>>()?;
    out.extend(derived_switch(runners.iter().map(|r| r.step.id.clone()).collect())?);
    if variables.iter().any(|v| v.name() == SEALED) {
        // The runner sets it once the machines are ready; on a server the internet is closed at
        // the router (`crate::server::router`).
        out.push(crate::module::manifest::seal_param(SEALED, "router")?);
    }
    Ok(out)
}

/// Variable `v` as a param the embedder sets: required when it has no default; a `bool` takes
/// `true` or `false`.
fn param(v: &Variable) -> Result<Param, ManifestError> {
    crate::module::manifest::param(v, false)
}

/// The connection's secrets: the sensitive ones of the module's `variables`.
pub(super) fn connection(variables: &[Variable]) -> Result<Vec<SecretSlot>, ManifestError> {
    variables
        .iter()
        .filter(|v| v.is_sensitive() && v.name() != INPUTS)
        .map(|v| slot(v.name(), SlotKind::Connection, true, Vec::new(), tf_var(v.name(), TfVarShape::String)?))
        .collect()
}

/// The connection's secrets (the module's sensitive variables), then each input a machine reads.
pub(super) fn secrets(env: &Environment) -> Result<Vec<SecretSlot>, ManifestError> {
    let mut out = connection(&super::super::base::variables(env))?;
    for input in &env.spec().inputs {
        let readers = crate::module::manifest::input_readers(env, input)?;
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
