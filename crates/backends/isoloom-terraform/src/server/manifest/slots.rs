//! What a server's run is given at run time: the module's variables as params (on Proxmox the
//! API, the node, the datastores, the uplink bridge, each Windows OS's template; on ESXi the host,
//! the datastore, the uplink port group, the OVAs), its sensitive ones (the passwords, the API
//! token) as connection slots, the lab's inputs as entries of the module's `inputs` map, the check
//! runners' switch, and the seal switch when the run seals. The runner sets three variables
//! itself, through the steps' environment (`super::steps::runner_set`).

use std::collections::BTreeSet;

use isoloom_manifest::{CheckRunner, Delivery, Error as ManifestError, Identifier, Name, NonEmpty, Param, SecretSlot, SlotKind, SnakeName, TfVarShape};
use isoloom_plan::Environment;

use isoloom_render::run_manifest::derived_switch;

use crate::module::variable::Variable;

/// The module's variable the inputs go in, one entry each.
const INPUTS: &str = "inputs";

/// The Terraform variable `name`.
fn tf_var(name: &str, shape: TfVarShape) -> Result<NonEmpty<Delivery>, ManifestError> {
    Ok(NonEmpty::one(Delivery::TfVar {
        var: SnakeName::parse(name)?,
        shape,
    }))
}

/// The module's variables an embedder sets (of `variables`, those the runner sets itself aside,
/// `private_key` among them: `super::steps::runner_set`), the checks' switch when the lab has
/// checks, and the seal switch when the run has an `internet: false` network to seal (`sealed`).
pub(crate) fn params(variables: &[Variable], private_key: &str, runners: &[CheckRunner], sealed: bool) -> Result<Vec<Param>, ManifestError> {
    let runner_set = super::steps::runner_set(private_key);
    let mut out: Vec<Param> = variables
        .iter()
        .filter(|v| !v.is_sensitive() && v.name() != INPUTS && !runner_set.iter().any(|(r, _)| *r == v.name()))
        .map(param)
        .collect::<Result<_, _>>()?;
    out.extend(derived_switch(runners.iter().map(|r| r.step.id.clone()).collect())?);
    if sealed {
        // The runner records it and replays it on every later op. The module has no such
        // variable (the router's cloud-init never changes, or the router would be made again), so
        // Terraform ignores it; the seal takes effect live through the seal-reload step on the
        // router (`crate::server::router`).
        out.push(crate::module::manifest::seal_param(crate::seal::SEALED, "router")?);
    }
    Ok(out)
}

/// Variable `v` as a param the embedder sets: required when it has no default; a `bool` takes
/// `true` or `false`.
fn param(v: &Variable) -> Result<Param, ManifestError> {
    crate::module::manifest::param(v, false)
}

/// The connection's secrets: the sensitive ones of the module's `variables`.
pub(crate) fn connection(variables: &[Variable]) -> Result<Vec<SecretSlot>, ManifestError> {
    variables
        .iter()
        .filter(|v| v.is_sensitive() && v.name() != INPUTS)
        .map(|v| slot(v.name(), SlotKind::Connection, true, Vec::new(), tf_var(v.name(), TfVarShape::String)?))
        .collect()
}

/// The connection's secrets (the module's sensitive `variables`), then each input a machine reads.
pub(super) fn secrets(env: &Environment, variables: &[Variable]) -> Result<Vec<SecretSlot>, ManifestError> {
    let mut out = connection(variables)?;
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
