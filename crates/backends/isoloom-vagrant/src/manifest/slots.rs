//! What a Vagrant run is given at run time: the params the Vagrantfile reads (the Hyper-V switch,
//! the SSH proxy, the ESXi host, whether derived checks run), the architecture Vagrant picks the
//! boxes' builds for on ESXi, and the slots of the values it must never hold: the inputs and
//! secrets its VMs get (read from the host's environment when Vagrant runs, by the steps that run
//! provisioners) and the ESXi password.

use std::collections::BTreeSet;

use isoloom_manifest::{
    CheckRunner, Delivery, EnvName, HostOs, Identifier, Lifecycle, Name, NonEmpty, Param, Provider, SecretSlot, SetBy, SlotKind, StepId, StepSelection,
};
use isoloom_plan::Environment;
use isoloom_spec::model::secret_variable;

use isoloom_render::run_manifest::derived_switch;

use super::lifecycle::{PROVISION, PROVISION_MACHINE, UP};
use crate::error::Result;
use crate::settings::{self, Setting};
use isoloom_manifest::contract;

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

/// The params the Vagrantfile reads, every step getting them; `ISOLOOM_DERIVED` only the check
/// runners'.
pub(super) fn params(runners: &[CheckRunner]) -> Result<Vec<Param>> {
    let mut out = vec![hyperv_switch()?, publish_fixed()?];
    out.extend(remote_params()?);
    out.extend(derived_switch(runners.iter().map(|r| r.step.id.clone()).collect())?);
    Ok(out)
}

/// Whether each published port takes its own `publish:` number on the host instead of a free one
/// the runner picks at the first start (an embedder's "use the lab's own ports" choice). Unset, the
/// runner picks free ports, so two VM labs forwarding one number never collide.
fn publish_fixed() -> Result<Param> {
    param(
        &Wanted {
            name: contract::PUBLISH_FIXED,
            description: "Forward each published port from its own `publish` number on the host, instead of a free port the runner picks.",
            default: None,
            values: &["1"],
            required: false,
            hosts: &[],
            providers: &[],
            set_by: SetBy::Embedder,
        },
        StepSelection::Every,
    )
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
    let spec = env.spec();
    let vms: Vec<(&String, &isoloom_plan::MachinePlan)> = env.machines.iter().filter(|(_, m)| m.shape == isoloom_plan::MachineShape::Vm).collect();
    let mut out = Vec::new();
    for input in &spec.inputs {
        let readers: Vec<&str> = vms.iter().filter(|(_, m)| m.reads_input(input)).map(|(n, _)| n.as_str()).collect();
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
        let Some(source) = isoloom_render::run_manifest::secret_source(source) else {
            continue;
        };
        let holders: Vec<&str> = vms
            .iter()
            .filter(|(_, m)| m.secrets.iter().any(|s| &s.name == name))
            .map(|(n, _)| n.as_str())
            .collect();
        if holders.is_empty() {
            continue;
        }
        let mut s = slot(name, SlotKind::Secret, &holders, env_var(&secret_variable(name), provisioning(lifecycle)?)?)?;
        s.source = Some(source);
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
