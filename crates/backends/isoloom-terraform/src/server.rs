//! The targets on a server of your own (the launcher's server mode): a Proxmox server
//! ([`proxmox`]) and a standalone ESXi host ([`esxi`]). Both put the environment behind a router
//! VM of their own ([`router`]), boot a machine's disk through the hypervisor's firmware
//! ([`boot`]), and name what they create after the environment's `slot` on the server
//! ([`slotted`]).

pub(crate) mod boot;
pub(crate) mod esxi;
pub(crate) mod proxmox;
pub(crate) mod router;

use isoloom_plan::{Capabilities, Environment, Feature, MachinePlan};
use isoloom_render::Error as RenderError;
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Files;
use isoloom_render::hcl_builder::{Block, Expression, Module, Template, object, reference, string};
use isoloom_spec::images::is_windows;
use isoloom_spec::model::Target;

use crate::machine::{self, controller, vms, windows};
use crate::module::providers::{self, Provider};
use crate::module::{ProjectCopy, Usage, inputs};
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

/// A server target's own parts, in the order its module writes them. The rest is the same on
/// both servers: the module's folder (the target's id), its first lines, its file, then the
/// check runners.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Parts {
    /// The target.
    pub(crate) target: Target,
    /// Why it can't run the environment, if it can't.
    pub(crate) refusal: fn(&Environment) -> Option<String>,
    /// How to start its module (what `apply` takes, what the host needs), from its folder.
    pub(crate) usage: for<'a> fn(&'a str) -> Usage<'a>,
    /// How its machines get the project.
    pub(crate) copy: ProjectCopy,
    /// Everything before the router: the providers, the variables, the connection, the
    /// networks.
    pub(crate) base: fn(&Environment, &mut Module),
    /// The router.
    pub(crate) router: fn(&Environment, &mut Module),
    /// The machines, and the controller.
    pub(crate) guests: fn(&Environment, &mut Module),
    /// The outputs.
    pub(crate) outputs: fn(&Environment, &mut Module),
}

impl Parts {
    /// The target's files, once the plan's requirements (`capabilities`) and its own refusal let
    /// it: its module, then the check runners.
    pub(crate) fn render(&self, capabilities: &Capabilities, env: &Environment) -> std::result::Result<Files, RenderError> {
        let target = self.target;
        crate::target::render(
            target,
            capabilities,
            env,
            || (self.refusal)(env),
            || {
                let mut files = vec![self.module(env)?];
                files.extend(checks::runners(env, target.id()));
                Ok(files)
            },
        )
    }

    /// Why the target can't run `env`: the plan's requirements it lacks (`capabilities`), else
    /// its own refusal.
    pub(crate) fn refusal(&self, capabilities: &Capabilities, env: &Environment) -> Option<String> {
        crate::target::refusal(env, capabilities, || (self.refusal)(env))
    }

    /// The module, part by part.
    fn module(&self, env: &Environment) -> Result<GeneratedFile> {
        let folder = self.target.id();
        let mut m = crate::module::start(&(self.usage)(folder));
        for part in [self.base, self.router, self.guests, self.outputs] {
            part(env, &mut m);
        }
        crate::module::file(self.target, folder, m, self.copy)
    }
}

/// A server module's start: the `terraform` block requiring the server's providers (`own`, and
/// `random` for the Windows machines' password), the server's `variables`, the `inputs`
/// variable, then its `provider`.
pub(crate) fn head(env: &Environment, m: &mut Module, own: &[Provider], variables: Vec<Block>, provider: Block) {
    let mut required = own.to_vec();
    if windows::any(env) {
        required.push(providers::RANDOM);
    }
    m.block(providers::terraform(&required));
    m.blocks(variables);
    m.blocks(inputs::variable(env));
    m.block(provider);
}

/// The first Linux VM whose OS has no image on the server (`has`: whether the server has one for
/// that OS and machine), with its OS.
pub(crate) fn without_image(env: &Environment, has: impl Fn(&str, &MachinePlan) -> bool) -> Option<(&str, &str)> {
    vms(env).find_map(|(name, mp)| {
        let os = mp.os.as_deref().unwrap_or_default();
        (!is_windows(os) && !has(os, mp)).then_some((name, os))
    })
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
