//! Each Windows VM's graphical console, as the cloud and server modules give it: its own Remote
//! Desktop, only when the spec opens its port (3389/tcp, which the set-up turns Remote Desktop
//! on for), reached through an SSH tunnel from this computer, never at a public address.
//!
//! The module gives three outputs, which a runner (Isothrum's tunnelled consoles) reads:
//!
//! - `windows_user`: the administrator Isoloom makes on its Windows VMs (`isoloom`);
//! - `windows_password`: their generated password, sensitive, so `terraform output` hides it;
//! - `consoles`: per Windows VM opening Remote Desktop, by its spec name, `{ protocol = "rdp",
//!   port = 3389, host, via, user }`: `host` the VM's address as the jump host reaches it, `via`
//!   the jump host's address this computer reaches, `user` the SSH user on the jump host.
//!
//! On a cloud the jump host is a Linux machine of the lab on a network the Windows one is on: the
//! first, in the lab's order, then the controller (on every network); a Windows VM no Linux
//! machine shares a network with has no entry (nothing refused: its Remote Desktop is only out
//! of the tunnel's reach). On a server it is the router, on every network. With no entry, the
//! `consoles` output is left out: a present one is the whole list.

use isoloom_plan::{Environment, Vm};

use crate::hcl_builder::{Block, Expression, Module, boolean, number, object, reference, string};
use crate::machine::windows::{ADMIN, PASSWORD};
use crate::module::output;

/// Remote Desktop's port.
pub(crate) const RDP_PORT: u16 = 3389;

/// The sensitive output holding the Windows machines' password, for Remote Desktop.
pub(crate) const PASSWORD_OUTPUT: &str = "windows_password";

/// The output naming the Windows machines' administrator.
const USER_OUTPUT: &str = "windows_user";

/// The output of the consoles a runner tunnels to.
const CONSOLES_OUTPUT: &str = "consoles";

/// Whether `vm` is a Windows machine whose spec opens Remote Desktop's port.
pub(crate) fn opens_rdp(env: &Environment, vm: Vm<'_>) -> bool {
    vm.plan.is_windows()
        && env
            .spec()
            .machines
            .get(vm.name)
            .is_some_and(|m| m.ports.iter().any(|p| p.port == RDP_PORT && p.protocol.is_tcp()))
}

/// The way to a Windows VM's Remote Desktop: through `via` as `user`, to `host`.
pub(crate) struct Tunnel {
    /// The jump host's address this computer reaches.
    pub(crate) via: Expression,
    /// The SSH user on the jump host.
    pub(crate) user: String,
    /// The VM's address as the jump host reaches it.
    pub(crate) host: String,
}

/// The console outputs ([`crate::consoles`]), each Windows VM opening Remote Desktop reached by
/// `tunnel` (`None`: no entry for it).
pub(crate) fn outputs(env: &Environment, module: &mut Module, tunnel: &dyn Fn(Vm<'_>) -> Option<Tunnel>) {
    let rdp: Vec<Vm<'_>> = env.vms().filter(|vm| opens_rdp(env, *vm)).collect();
    if rdp.is_empty() {
        return;
    }
    module.commented(
        "The Windows machines' administrator password (user isoloom), for their Remote Desktop.",
        Block::builder("output")
            .add_label(PASSWORD_OUTPUT)
            .add_attribute(("value", reference(PASSWORD)))
            .add_attribute(("sensitive", boolean(true)))
            .build(),
    );
    module.commented("The Windows machines' administrator.", output(USER_OUTPUT, string(ADMIN)));
    let entries: Vec<(&str, Expression)> = rdp
        .iter()
        .filter_map(|vm| {
            let t = tunnel(*vm)?;
            Some((
                vm.name,
                object([
                    ("protocol", string("rdp")),
                    ("port", number(RDP_PORT.into())),
                    ("host", string(t.host)),
                    ("via", t.via),
                    ("user", string(t.user)),
                ]),
            ))
        })
        .collect();
    if !entries.is_empty() {
        module.commented(
            "Each Windows machine's Remote Desktop, through an SSH tunnel: ssh -L <port>:<host>:3389 <user>@<via>.",
            output(CONSOLES_OUTPUT, object(entries)),
        );
    }
}

/// The first of `guests` (Linux machines by name, in the lab's order; `controller`, when given,
/// last and on every network) on a network `vm` is on, with `vm`'s address there.
pub(crate) fn jump<'g>(env: &Environment, vm: Vm<'_>, guests: &[&'g str], controller: Option<&'g str>) -> Option<(&'g str, String)> {
    let shared = |guest: &str| {
        vm.plan
            .addresses
            .iter()
            .find(|(net, _)| env.machines.get(guest).is_some_and(|mp| mp.addresses.iter().any(|(n, _)| n == *net)))
            .map(|(_, ip)| ip.to_string())
    };
    guests
        .iter()
        .find_map(|g| shared(g).map(|host| (*g, host)))
        .or_else(|| controller.and_then(|c| vm.plan.addresses.values().next().map(|ip| (c, ip.to_string()))))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "version: 1
name: shop
networks: { lab: { cidr: 10.9.0.0/24 }, back: { cidr: 10.8.0.0/24 } }
machines:
  web: { networks: { lab: 10 }, os: debian@12 }
  dc: { networks: { lab: 20 }, os: windows-server@2022, ports: { rdp: { port: 3389 } } }
  alone: { networks: { back: 30 }, os: windows-server@2022, ports: { rdp: { port: 3389 } } }
  plain: { networks: { lab: 21 }, os: windows-server@2022 }
";

    fn rendered(env: &Environment) -> String {
        let mut m = Module::default();
        outputs(env, &mut m, &|vm| {
            let (guest, host) = jump(env, vm, &["web"], None)?;
            Some(Tunnel {
                via: reference(&format!("local.{guest}")),
                user: "admin".into(),
                host,
            })
        });
        m.render().unwrap()
    }

    #[test]
    fn a_windows_vm_opening_rdp_is_tunnelled_through_a_linux_one_beside_it() {
        let env = crate::testing::environment(SPEC, isoloom_spec::model::Target::CloudVm);
        let text = rendered(&env);
        assert!(text.contains("output \"windows_password\"") && text.contains("sensitive = true"), "{text}");
        assert!(text.contains("output \"windows_user\"") && text.contains("\"isoloom\""), "{text}");
        assert!(
            text.contains("dc = {") && text.contains("\"10.9.0.20\"") && text.contains("local.web"),
            "{text}"
        );
        assert!(text.contains("\"rdp\"") && text.contains("3389"), "{text}");
        assert!(!text.contains("alone"), "no Linux machine beside it: no entry: {text}");
        assert!(!text.contains("plain"), "its spec opens no Remote Desktop port: {text}");
    }

    #[test]
    fn without_an_entry_the_consoles_output_is_left_out() {
        let env = crate::testing::environment(
            &SPEC.replace("  dc:", "  dc2:").replace("networks: { lab: 20 }", "networks: { back: 20 }"),
            isoloom_spec::model::Target::CloudVm,
        );
        let text = rendered(&env);
        assert!(text.contains("output \"windows_user\"") && !text.contains("output \"consoles\""), "{text}");
        let none = crate::testing::environment(
            &SPEC.replace("ports: { rdp: { port: 3389 } }", "ports: { web: { port: 80 } }"),
            isoloom_spec::model::Target::CloudVm,
        );
        assert_eq!(rendered(&none), "");
    }
}
