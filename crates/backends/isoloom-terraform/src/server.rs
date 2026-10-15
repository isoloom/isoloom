//! The targets on a server of your own (the launcher's server mode): a Proxmox server
//! ([`proxmox`]) and a standalone ESXi host ([`esxi`]). Both put the environment behind a router
//! VM of their own ([`router`]), boot a machine's disk through the hypervisor's firmware
//! ([`boot`]), and name what they create after the environment's `slot` on the server
//! ([`slotted`]).

pub(crate) mod boot;
pub(crate) mod esxi;
pub(crate) mod proxmox;
pub(crate) mod router;

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Expression, Module, Template, object, reference, string};

use crate::machine::{controller, windows};

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
