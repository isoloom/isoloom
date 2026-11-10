//! What a Vagrant run is given at run time: the params the Vagrantfile reads (the Hyper-V switch,
//! the SSH proxy, the ESXi host, whether derived checks run), the architecture Vagrant picks the
//! boxes' builds for on ESXi, and the slots of the values it must never hold: the inputs and
//! secrets its VMs get (read from the host's environment when Vagrant runs, by the steps that run
//! provisioners) and the ESXi password.

use std::collections::BTreeSet;

use isoloom_manifest::contract::secret_variable;
use isoloom_manifest::{
    CheckRunner, Delivery, EnvName, HostOs, Identifier, Lifecycle, Machine, Name, NonEmpty, Param, Provider, SecretSlot, SetBy, SlotKind, StepId, StepSelection,
};
use isoloom_plan::{Environment, MachineShape};

use isoloom_render::run_manifest::{derived_switch, held_secrets, publish_fixed};

use super::lifecycle::{PROVISION, PROVISION_MACHINE, UP};
use crate::error::Result;
use crate::settings::{self, Setting};

/// A param: what it is, its default, the values it takes, and whom it concerns.
struct Wanted<'a> {
    name: &'a str,
    description: &'a str,
    default: Option<String>,
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
        default: p.default.clone(),
        values: p.values.iter().map(|v| (*v).to_owned()).collect(),
        required: p.required,
        hosts: p.hosts.iter().copied().collect(),
        providers: p.providers.iter().copied().collect(),
        set_by: p.set_by,
        deliver: env_var(p.name, steps)?,
    })
}

/// What the Vagrantfile reads of ESXi's host (vagrant-vmware-esxi), besides its password.
const ESXI: [(Setting, &str, bool); 5] = [
    (settings::ESXI_HOSTNAME, "The ESXi host.", true),
    (settings::ESXI_HOSTPORT, "Its SSH port.", false),
    (settings::ESXI_USERNAME, "The ESXi user.", false),
    (settings::ESXI_DATASTORE, "The datastore the VMs' disks go on (default: ESXi's first).", false),
    (
        settings::ESXI_VIRTUAL_NETWORK,
        "Port groups, comma-separated: the management network, then one per environment network.",
        false,
    ),
];

/// The params the Vagrantfile reads, every step getting them: whether the published ports of
/// `machines` keep their own numbers when one is published (unset, the runner picks free host
/// ports at the first start, so two environments forwarding one number never collide), and
/// `ISOLOOM_DERIVED` only the check runners'.
pub(super) fn params(machines: &[Machine], runners: &[CheckRunner]) -> Result<Vec<Param>> {
    let mut out = vec![hyperv_switch()?];
    out.extend(publish_fixed(machines)?);
    out.extend(remote_params()?);
    out.extend(derived_switch(runners.iter().map(|r| r.step.id.clone()).collect())?);
    Ok(out)
}

/// The Hyper-V switch the VMs' first adapter goes on.
fn hyperv_switch() -> Result<Param> {
    param(
        &Wanted {
            name: settings::HYPERV_SWITCH.name,
            description: "The Hyper-V virtual switch each VM's first adapter (Vagrant's connection, and the internet) goes on.",
            default: settings::HYPERV_SWITCH.default(),
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
            name: settings::SSH_PROXY_COMMAND.name,
            description: "The command Vagrant's SSH to the VMs goes through (an SSH `ProxyCommand`), when this computer can't reach them directly; unset, it connects straight to them.",
            default: settings::SSH_PROXY_COMMAND.default(),
            values: &[],
            required: false,
            hosts: &[],
            providers: &[Provider::VmwareEsxi],
            set_by: SetBy::Runner,
        },
        StepSelection::Every,
    )?];
    for (setting, description, required) in ESXI {
        let spec = Wanted {
            name: setting.name,
            description,
            default: setting.default(),
            values: &[],
            required,
            hosts: &[],
            providers: &[Provider::VmwareEsxi],
            set_by: SetBy::Embedder,
        };
        out.push(param(&spec, StepSelection::Every)?);
    }
    out.push(esxi_architecture()?);
    Ok(out)
}

/// The architecture Vagrant takes for the computer it runs on, which picks each box's build
/// (`box_architecture`'s default, `:auto`): on ESXi, the ESXi host's, `amd64`. Vagrant itself
/// runs here, but the VMs run on the ESXi host, which is x86-64: from an Apple Silicon Mac,
/// Vagrant would otherwise take a box's arm64 build and upload it. Vagrant's own variable, not
/// `box_architecture` in the provider's block: Vagrant looks a machine's box up before it applies
/// a provider's overrides, so the box it found for this computer would stay; and the variable
/// keeps Vagrant's fallback to the one build of a box that names no architecture (one published
/// before Vagrant 2.4), which an explicit `amd64` refuses. An `arm64` machine is refused on ESXi.
fn esxi_architecture() -> Result<Param> {
    const VAR: &str = "VAGRANT_HOST_ARCHITECTURE";
    let esxi = crate::vms::provider::ESXI_ARCH.id();
    Ok(Param {
        name: EnvName::parse(VAR)?,
        description: "The architecture Vagrant picks each box's build for: the ESXi host's (x86-64), not this computer's.".into(),
        default: Some(esxi.to_owned()),
        values: vec![esxi.to_owned()],
        required: false,
        hosts: BTreeSet::new(),
        providers: BTreeSet::from([Provider::VmwareEsxi]),
        set_by: SetBy::Runner,
        deliver: env_var(VAR, StepSelection::Every)?,
    })
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
    let vms: Vec<(&String, &isoloom_plan::MachinePlan)> = env.machines.iter().filter(|(_, m)| m.shape == MachineShape::Vm).collect();
    let mut out = Vec::new();
    for input in &env.inputs {
        let readers: Vec<&str> = vms.iter().filter(|(_, m)| m.reads_input(&input.name)).map(|(n, _)| n.as_str()).collect();
        if readers.is_empty() {
            continue;
        }
        let mut s = slot(&input.name, SlotKind::Input, &readers, env_var(&input.name, provisioning(lifecycle)?)?)?;
        s.sensitive = input.sensitive;
        // An input may be left out: the Vagrantfile reads it as empty, and so does the runner.
        s.required = false;
        s.default = Some(String::new());
        out.push(s);
    }
    for held in held_secrets(env, MachineShape::Vm) {
        let deliver = env_var(&secret_variable(held.name), provisioning(lifecycle)?)?;
        let mut s = slot(held.name, SlotKind::Secret, &held.holders, deliver)?;
        s.source = Some(held.source);
        out.push(s);
    }
    out.push(esxi_password()?);
    Ok(out)
}

/// The ESXi host's password, given to every step on `vmware_esxi`.
pub(super) fn esxi_password() -> Result<SecretSlot> {
    let name = settings::ESXI_PASSWORD.name;
    let mut password = slot(name, SlotKind::Connection, &[], env_var(name, StepSelection::Every)?)?;
    password.providers = BTreeSet::from([Provider::VmwareEsxi]);
    Ok(password)
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
