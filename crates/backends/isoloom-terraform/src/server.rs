//! The targets on a server of your own (the launcher's server mode): a Proxmox server
//! ([`proxmox`]) and a standalone ESXi host ([`esxi`]). Both put the environment behind a router
//! VM of their own ([`router`]), boot a machine's disk through the hypervisor's firmware
//! ([`boot`]), and name what they create after the environment's `slot` on the server
//! ([`slotted`]).

pub(crate) mod boot;
pub(crate) mod esxi;
pub(crate) mod proxmox;
pub(crate) mod router;

use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::GeneratedFile;
use isoloom_render::backend::{Files, RenderError};
use isoloom_render::hcl_builder::{Expression, Module, Template, object, reference, string};
use isoloom_spec::model::Target;

use crate::machine::{self, controller, windows};
use crate::{Result, checks};

/// What a server target renders: the VM core's features (`machine::FEATURES`), and networks
/// outside the environment (an existing bridge or port group, `external`, or the server's LAN,
/// `bridged`) and a machine's own firmware (`vm.boot`, each target refusing what its hypervisor
/// has no setting for).
pub(crate) fn features() -> impl Iterator<Item = Feature> {
    machine::FEATURES
        .into_iter()
        .chain([Feature::ExternalNetworks, Feature::BridgedNetworks, Feature::OwnBoot])
}

/// A server target's files, once the plan's requirements (`capabilities`) and its own `refusal`
/// let it: its `module`, then the check runners.
pub(crate) fn render(
    target: Target,
    capabilities: &Capabilities,
    env: &Environment,
    refusal: fn(&Environment) -> Option<String>,
    module: fn(&Environment) -> Result<GeneratedFile>,
) -> std::result::Result<Files, RenderError> {
    crate::target::render(
        target,
        capabilities,
        env,
        || refusal(env),
        || {
            let mut files = vec![module(env)?];
            files.extend(checks::runners(env, target.id()));
            Ok(files)
        },
    )
}

/// `iso<slot><suffix>`: a name unique per environment on the server (`var.slot`).
pub(crate) fn slotted(suffix: &str) -> Expression {
    Template::text("iso").with_expr(reference("var.slot")).with_text(suffix).into_expression()
}

/// The `isoloom` user cloud-init creates on a server's VMs, with `sudo` and the SSH keys
/// `keys`.
pub(crate) fn user(keys: Expression) -> Expression {
    object([
        ("name", string("isoloom")),
        ("sudo", string("ALL=(ALL) NOPASSWD:ALL")),
        ("shell", string("/bin/bash")),
        ("ssh_authorized_keys", keys),
    ])
}

/// The credentials a server's module generates: the controller's key, then the Windows
/// machines' password.
pub(crate) fn credentials(env: &Environment, m: &mut Module) {
    controller::write_key(env, m);
    windows::write_password(env, m);
}
