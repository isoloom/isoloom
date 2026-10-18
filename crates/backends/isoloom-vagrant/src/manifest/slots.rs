//! What a Vagrant run is given at run time: the params the Vagrantfile reads (the Hyper-V switch,
//! the SSH proxy, the ESXi host, whether derived checks run), and the slots of the values it must
//! never hold: the inputs and secrets its VMs get (read from the host's environment when Vagrant
//! runs, by the steps that run provisioners) and the ESXi password.

use std::collections::BTreeSet;

use isoloom_manifest::{
    CheckRunner, Delivery, EnvName, HostOs, Identifier, Lifecycle, Name, NonEmpty, Param, Provider, SecretSlot, SecretSource, SetBy, SlotKind, StepId,
    StepSelection, Store,
};
use isoloom_plan::{Environment, ValueSource};
use isoloom_spec::model::{SecretSource as SpecSource, secret_variable};

use isoloom_render::run_manifest::derived_switch;

use super::lifecycle::{PROVISION, PROVISION_MACHINE, UP};
use crate::error::Result;

/// A param: what it is, its default, the values it takes, and whom it concerns.
struct Wanted<'a> {
    name: &'a str,
    description: &'a str,
    default: Option<&'a str>,
    values: &'a [&'a str],
    required: bool,
    hosts: &'a [HostOs],
    providers: &'a [Provider],
    set_by: SetBy,
}

/// The variable `var` of `steps`.
fn env_var(var: &str, steps: StepSelection) -> Result<NonEmpty<Delivery>> {
    Ok(NonEmpty::one(Delivery::Env {
        var: EnvName::parse(var)?,
        steps,
    }))
}

fn param(p: &Wanted<'_>, steps: StepSelection) -> Result<Param> {
    Ok(Param {
        name: EnvName::parse(p.name)?,
        description: p.description.into(),
        default: p.default.map(String::from),
        values: p.values.iter().map(|v| (*v).to_owned()).collect(),
        required: p.required,
        hosts: p.hosts.iter().copied().collect(),
        providers: p.providers.iter().copied().collect(),
        set_by: p.set_by,
        deliver: env_var(p.name, steps)?,
    })
}

/// What the Vagrantfile reads of ESXi's host (vagrant-vmware-esxi), besides its password.
const ESXI: [(&str, &str, Option<&str>, bool); 5] = [
    ("ESXI_HOSTNAME", "The ESXi host.", None, true),
    ("ESXI_HOSTPORT", "Its SSH port.", Some("22"), false),
    ("ESXI_USERNAME", "The ESXi user.", Some("root"), false),
    ("ESXI_DATASTORE", "The datastore the VMs' disks go on (default: ESXi's first).", None, false),
    (
        "ESXI_VIRTUAL_NETWORK",
        "Port groups, comma-separated: the management network, then one per lab network.",
        Some("VM Network"),
        false,
    ),
];

/// The params the Vagrantfile reads, every step getting them; `ISOLOOM_DERIVED` only the check
/// runners'.
pub(super) fn params(runners: &[CheckRunner]) -> Result<Vec<Param>> {
    let mut out = vec![hyperv_switch()?];
    out.extend(remote_params()?);
    out.extend(derived_switch(runners.iter().map(|r| r.step.id.clone()).collect())?);
    Ok(out)
}

/// The Hyper-V switch the VMs' first adapter goes on.
fn hyperv_switch() -> Result<Param> {
    param(
        &Wanted {
            name: "ISOLOOM_HYPERV_SWITCH",
            description: "The Hyper-V virtual switch each VM's first adapter (Vagrant's connection, and the internet) goes on.",
            default: Some("Default Switch"),
            values: &[],
            required: false,
            hosts: &[HostOs::Windows],
            providers: &[Provider::Hyperv],
            set_by: SetBy::Embedder,
        },
        StepSelection::Every,
    )
}

/// What a VM on a remote host needs: the SSH proxy through the runner's relay, and the ESXi
/// host's settings (its password is a slot, [`esxi_password`]).
pub(super) fn remote_params() -> Result<Vec<Param>> {
    let mut out = vec![param(
        &Wanted {
            name: "ISOLOOM_SSH_PROXY_COMMAND",
            description: "The command Vagrant's SSH to the VMs goes through when this host can't reach them directly (an ESXi host through the runner's relay on macOS).",
            default: None,
            values: &[],
            required: false,
            hosts: &[],
            providers: &[Provider::VmwareEsxi],
            set_by: SetBy::Runner,
        },
        StepSelection::Every,
    )?];
    for (name, description, default, required) in ESXI {
        let spec = Wanted {
            name,
            description,
            default,
            values: &[],
            required,
            hosts: &[],
            providers: &[Provider::VmwareEsxi],
            set_by: SetBy::Embedder,
        };
        out.push(param(&spec, StepSelection::Every)?);
    }
    Ok(out)
}

/// The steps that run the VMs' provisioners, which read the values given at launch: `up`, and
/// the provision steps there are.
fn provisioning(lifecycle: &Lifecycle) -> Result<StepSelection> {
    let provision = lifecycle.provision.iter().flat_map(|p| p.all.iter().chain(&p.machine));
    let ids: BTreeSet<&str> = provision
        .map(|s| s.id.as_str())
        .filter(|id| [PROVISION, PROVISION_MACHINE].contains(id))
        .collect();
    let steps = std::iter::once(UP).chain(ids).map(StepId::parse).collect::<Result<_, _>>()?;
    Ok(StepSelection::Only(NonEmpty::new(steps, "the provisioning steps")?))
}

/// The slots: each input a VM reads (as its environment or a file; delivered empty when left
/// out), each secret a VM gets (from its store), and ESXi's password.
pub(super) fn slots(env: &Environment, lifecycle: &Lifecycle) -> Result<Vec<SecretSlot>> {
    let spec = env.spec();
    let vms: Vec<(&String, &isoloom_plan::MachinePlan)> = env.machines.iter().filter(|(_, m)| m.shape == isoloom_plan::MachineShape::Vm).collect();
    let mut out = Vec::new();
    for input in &spec.inputs {
        let file = |m: &isoloom_plan::MachinePlan| m.secrets.iter().any(|s| s.source == ValueSource::Input(input.clone()));
        let readers: Vec<&str> = vms
            .iter()
            .filter(|(_, m)| m.inputs.contains(input) || file(m))
            .map(|(n, _)| n.as_str())
            .collect();
        if readers.is_empty() {
            continue;
        }
        let mut s = slot(input, SlotKind::Input, &readers, env_var(input, provisioning(lifecycle)?)?)?;
        // The spec's own word on it; an input it says nothing of is masked.
        s.sensitive = !spec.plain_inputs.contains(input);
        // An input may be left out: the Vagrantfile reads it as empty, and so does the runner.
        s.required = false;
        s.default = Some(String::new());
        out.push(s);
    }
    for (name, source) in &spec.secrets {
        let Some(store) = store(source) else { continue };
        let holders: Vec<&str> = vms
            .iter()
            .filter(|(_, m)| m.secrets.iter().any(|s| &s.name == name))
            .map(|(n, _)| n.as_str())
            .collect();
        if holders.is_empty() {
            continue;
        }
        let mut s = slot(name, SlotKind::Secret, &holders, env_var(&secret_variable(name), provisioning(lifecycle)?)?)?;
        s.source = Some(SecretSource {
            store,
            reference: source.reference().to_owned(),
        });
        out.push(s);
    }
    out.push(esxi_password()?);
    Ok(out)
}

/// The ESXi host's password, given to every step on `vmware_esxi`.
pub(super) fn esxi_password() -> Result<SecretSlot> {
    let mut password = slot("ESXI_PASSWORD", SlotKind::Connection, &[], env_var("ESXI_PASSWORD", StepSelection::Every)?)?;
    password.providers = BTreeSet::from([Provider::VmwareEsxi]);
    Ok(password)
}

/// A secret's store; an input's file has none (the input's slot carries it).
fn store(s: &SpecSource) -> Option<Store> {
    match s {
        SpecSource::Env { .. } => Some(Store::Env),
        SpecSource::File { .. } => Some(Store::File),
        SpecSource::Vault { .. } => Some(Store::Vault),
        SpecSource::Sops { .. } => Some(Store::Sops),
        SpecSource::Input { .. } => None,
    }
}

/// A required, sensitive slot named `name`, for `machines`, delivered as `deliver`.
fn slot(name: &str, kind: SlotKind, machines: &[&str], deliver: NonEmpty<Delivery>) -> Result<SecretSlot> {
    Ok(SecretSlot {
        name: Identifier::parse(name)?,
        kind,
        required: true,
        sensitive: true,
        machines: machines.iter().map(|m| Name::parse(m)).collect::<Result<_, _>>()?,
        providers: BTreeSet::new(),
        source: None,
        default: None,
        deliver,
    })
}
