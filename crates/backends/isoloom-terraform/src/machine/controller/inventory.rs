//! The controller's Ansible inventory: how it reaches each VM of Isoloom's server targets and
//! clouds (SSH with its key, WinRM with the generated Windows password).

use isoloom_plan::{Environment, Vm};
use isoloom_render::inventory::{self, Access, CONTROLLER_KEY, Vars, vars};
use isoloom_spec::model::Winrm;

use crate::hcl_builder::{Template, reference};
use crate::machine::windows;

/// A cloud's inventory (the shared one, `isoloom_render::inventory`): each Linux VM as its
/// image's user (`user(os)`), the Windows VMs over WinRM (authenticated with `auth`, as
/// Terraform's own connection is) with the generated password.
pub(crate) fn cloud_inventory(env: &Environment, user: impl Fn(&str) -> String, auth: WinrmAuth) -> Template {
    inventory_template(env, |password| cloud_access(user, password, auth))
}

/// How WinRM authenticates over plain HTTP: Basic where the image allows it, NTLM where it
/// doesn't (Azure's Windows images).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WinrmAuth {
    /// Basic authentication (the user data allows it).
    Basic,
    /// NTLM.
    Ntlm,
}

/// How a Windows machine set up by Isoloom (on the servers and the clouds) is reached: as
/// `isoloom` with a generated password, over WinRM on plain HTTP.
fn isoloom_over_winrm(password: &str, transport: WinrmAuth) -> Vars {
    let mut windows = vars(&[("ansible_user", "isoloom"), ("ansible_password", password), ("ansible_connection", "winrm")]);
    windows.extend(match transport {
        WinrmAuth::Basic => inventory::winrm_vars(Winrm::Plaintext),
        WinrmAuth::Ntlm => vars(&[("ansible_port", "5985"), ("ansible_winrm_scheme", "http"), ("ansible_winrm_transport", "ntlm")]),
    });
    windows.extend(vars(&inventory::WINRM_PATIENCE));
    windows
}

/// How the controller reaches the VMs of Isoloom's server targets (Proxmox, ESXi): the `isoloom`
/// user cloud-init creates on each Linux VM, with the controller's key; on a Windows VM, the
/// `isoloom` administrator its set-up makes, whose password is `windows_password` (a Terraform
/// expression, as the module writes it), over WinRM.
pub(crate) fn server_access<'a>(windows_password: &str) -> Access<'a> {
    Access::new(
        vars(&[("ansible_ssh_private_key_file", CONTROLLER_KEY)]),
        isoloom_over_winrm(windows_password, WinrmAuth::Basic),
        |h| {
            if h.machine.is_windows() {
                Vars::new()
            } else {
                vars(&[("ansible_user", "isoloom")])
            }
        },
    )
}

/// How the controller reaches a cloud's VMs: each Linux VM as its image's user (`linux_user` of
/// its OS) with the controller's key; Windows over plain-HTTP WinRM as `isoloom`, whose password
/// is `windows_password` (a Terraform expression), authenticated as the cloud's images allow
/// (`auth`, as Terraform's own connection does).
pub(crate) fn cloud_access<'a>(linux_user: impl Fn(&str) -> String + 'a, windows_password: &str, auth: WinrmAuth) -> Access<'a> {
    Access::new(
        vars(&[("ansible_ssh_private_key_file", CONTROLLER_KEY)]),
        isoloom_over_winrm(windows_password, auth),
        move |h| match Vm::of(h.name, h.machine) {
            Some(vm) if !vm.plan.is_windows() => vec![("ansible_user".to_owned(), linux_user(vm.os))],
            _ => Vars::new(),
        },
    )
}

/// The inventory `access` gives (from the generated Windows password's Terraform expression),
/// that password interpolated where the inventory names it.
pub(crate) fn inventory_template<'a>(env: &Environment, access: impl FnOnce(&str) -> Access<'a>) -> Template {
    let marker = format!("${{{}}}", windows::PASSWORD);
    let text = inventory::render(env, &access(&marker));
    let mut t = Template::new();
    for (i, part) in text.split(marker.as_str()).enumerate() {
        if i > 0 {
            t.push_expr(reference(windows::PASSWORD));
        }
        t.push_text(part);
    }
    t
}
