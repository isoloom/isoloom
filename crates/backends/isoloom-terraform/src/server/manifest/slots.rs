//! What a server's run is given at run time: the module's variables as params (on Proxmox the
//! API, the node, the datastores, the uplink bridge, each Windows OS's template; on ESXi the host,
//! the datastore, the uplink port group, the OVAs), its sensitive ones (the passwords, the API
//! token) as connection slots, the environment's inputs as entries of the module's `inputs` map, the check
//! runners' switch, and the seal switch when the run seals. The runner sets three variables
//! itself, through the steps' environment (`super::steps::runner_set`).

use std::collections::BTreeSet;

use isoloom_manifest::{
    CheckRunner, Delivery, EnvName, Error as ManifestError, Identifier, Name, NonEmpty, Param, SecretSlot, SlotKind, SnakeName, StepSelection, TfVarShape,
};
use isoloom_plan::Environment;

use isoloom_render::run_manifest::derived_switch;

use crate::module::variable::Variable;

/// The Terraform variable `name`.
fn tf_var(name: &str, shape: TfVarShape) -> Result<NonEmpty<Delivery>, ManifestError> {
    Ok(NonEmpty::one(Delivery::TfVar {
        var: SnakeName::parse(name)?,
        shape,
    }))
}

/// The module's variables an embedder sets (of `variables`, those the runner sets itself aside,
/// `private_key` among them: `super::steps::runner_set`), and the checks' switch when the
/// environment has checks.
pub(crate) fn params(variables: &[Variable], private_key: &str, runners: &[CheckRunner]) -> Result<Vec<Param>, ManifestError> {
    let runner_set = super::steps::runner_set(private_key);
    let mut out: Vec<Param> = variables
        .iter()
        .filter(|v| !v.is_sensitive() && !runner_set.iter().any(|(r, _)| *r == v.name()))
        .map(param)
        .collect::<Result<_, _>>()?;
    out.extend(derived_switch(runners.iter().map(|r| r.step.id.clone()).collect())?);
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
        .filter(|v| v.is_sensitive())
        .map(|v| slot(v.name(), SlotKind::Connection, true, Vec::new(), tf_var(v.name(), TfVarShape::String)?))
        .collect()
}

/// The connection's secrets (the module's sensitive `variables`), then each input a machine reads.
pub(super) fn secrets(env: &Environment, variables: &[Variable]) -> Result<Vec<SecretSlot>, ManifestError> {
    let mut out = connection(variables)?;
    for input in &env.inputs {
        // The machines reading it in their environment or as a file.
        let readers: Vec<Name> = env
            .vms()
            .filter(|vm| vm.plan.reads_input(&input.name))
            .map(|vm| Name::parse(vm.name))
            .collect::<Result<_, _>>()?;
        if readers.is_empty() {
            continue;
        }
        let var = EnvName::parse(&format!("TF_VAR_{}", input.name))?;
        let deliver = NonEmpty::one(Delivery::Env {
            var,
            steps: StepSelection::Every,
        });
        let mut s = slot(&input.name, SlotKind::Input, input.sensitive, readers, deliver)?;
        // Left out, an input is an empty value in the map: the machines read it empty.
        s.default = Some(String::new());
        out.push(s);
    }
    // Each secret a machine gets as a file, from its store: the module's variable
    // `ISOLOOM_SECRET_<NAME>`, which its set-up writes in place.
    for held in isoloom_render::run_manifest::held_secrets(env, isoloom_plan::MachineShape::Vm) {
        let readers: Vec<Name> = held.holders.iter().map(|n| Name::parse(n)).collect::<Result<_, _>>()?;
        out.push(crate::cloud::manifest::connection::machine_secret(held.name, held.source, &readers)?);
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
