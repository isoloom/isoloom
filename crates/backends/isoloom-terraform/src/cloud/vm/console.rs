//! Each cloud VM's graphical console. A cloud gives none of its own that Isoloom reaches (a
//! serial console at most), so a Windows VM's is its own Remote Desktop, and only when the spec
//! opens its port (3389/tcp, which the set-up turns Remote Desktop on for): the runner reaches it
//! through an SSH tunnel from this computer's loopback, through a Linux machine of the run on a
//! network the Windows one is on, never at a public address. It signs in as the administrator
//! Isoloom makes (`isoloom`), with the generated password the module gives as a sensitive output
//! ([`PASSWORD_OUTPUT`]). A Linux VM has none: its way in is SSH.

use std::collections::BTreeSet;

use isoloom_manifest::{
    Console, ConsoleAccount, ConsoleProtocol, ConsoleSource, Error as ManifestError, Identifier, Machine, Output, OutputRole, OutputSource, Placeholder,
    SnakeName, Template,
};
use isoloom_plan::{Environment, Vm};

use crate::cloud::manifest::Guest;
use crate::hcl_builder::{Block, Module, boolean, reference};
use crate::machine::windows::{ADMIN, PASSWORD};

/// Remote Desktop's port.
pub(crate) const RDP_PORT: u16 = 3389;

/// The sensitive output holding the Windows machines' password, for Remote Desktop.
pub(crate) const PASSWORD_OUTPUT: &str = "windows_password";

/// Why a Linux cloud VM has no graphical console.
const LINUX: &str = "a Linux cloud VM has no graphical console Isoloom offers: its way in is SSH";

/// Why a Windows VM whose spec doesn't open Remote Desktop's port has none.
const NO_PORT: &str = "its spec opens no Remote Desktop port (3389/tcp) for the runner's tunnel to reach";

/// Why a Windows VM with no Linux machine beside it has none.
const NO_JUMP: &str = "no Linux machine of the run shares a network with it, to tunnel Remote Desktop through";

/// Whether `vm` is a Windows machine whose spec opens Remote Desktop's port.
pub(crate) fn opens_rdp(env: &Environment, vm: Vm<'_>) -> bool {
    vm.plan.is_windows()
        && env
            .spec()
            .machines
            .get(vm.name)
            .is_some_and(|m| m.ports.iter().any(|p| p.port == RDP_PORT && p.protocol.is_tcp()))
}

/// The module's output of the Windows machines' password, when a Windows machine's Remote
/// Desktop is reached: sensitive, so `terraform output` hides it and the runner masks it.
pub(crate) fn outputs(env: &Environment, m: &mut Module) {
    if env.vms().any(|vm| opens_rdp(env, vm)) {
        m.commented(
            "The Windows machines' administrator password (user isoloom), for their Remote Desktop.",
            Block::builder("output")
                .add_label(PASSWORD_OUTPUT)
                .add_attribute(("value", reference(PASSWORD)))
                .add_attribute(("sensitive", boolean(true)))
                .build(),
        );
    }
}

/// Each VM's console among `machines`, through one of `guests` (the run's Linux machines and its
/// controller, which the runner's key signs in to), and the output a console reads.
pub(crate) fn add(env: &Environment, guests: &[Guest], machines: &mut [Machine], outputs: &mut Vec<Output>) -> Result<(), ManifestError> {
    let all = BTreeSet::new;
    let mut rdp = false;
    for m in machines.iter_mut() {
        let Some(vm) = env.vm(m.name.as_str()) else { continue };
        let console = if !vm.plan.is_windows() {
            Console::none(all(), LINUX)
        } else if !opens_rdp(env, vm) {
            Console::none(all(), NO_PORT)
        } else if let Some((guest, address)) = jump(env, vm, guests) {
            rdp = true;
            let source = ConsoleSource::SshTunnel {
                via: guest.login.host.clone(),
                user: Template::text(&guest.login.user),
                host: Template::text(&address),
                port: RDP_PORT,
            };
            let mut c = Console::at(all(), ConsoleProtocol::Rdp, source);
            c.account = Some(ConsoleAccount {
                user: Template::text(ADMIN),
                password: Template::from(Placeholder::Output(SnakeName::parse(PASSWORD_OUTPUT)?)),
            });
            c
        } else {
            Console::none(all(), NO_JUMP)
        };
        m.consoles = vec![console];
    }
    if rdp {
        outputs.push(Output {
            name: SnakeName::parse(PASSWORD_OUTPUT)?,
            description: Some("The Windows machines' administrator password (user isoloom), for their Remote Desktop.".into()),
            role: OutputRole::Info,
            sensitive: true,
            format: None,
            from: OutputSource::TerraformOutput {
                name: Identifier::parse(PASSWORD_OUTPUT)?,
                default: None,
            },
        });
    }
    Ok(())
}

/// The guest the tunnel to `vm` goes through, and `vm`'s address on the network they share: the
/// first guest, in the run's order, on one of its networks.
fn jump<'g>(env: &Environment, vm: Vm<'_>, guests: &'g [Guest]) -> Option<(&'g Guest, String)> {
    guests.iter().find_map(|g| {
        let on = |net: &str| {
            if g.machine == crate::machine::controller::NAME {
                env.networks.contains_key(net)
            } else {
                env.machines.get(&g.machine).is_some_and(|mp| mp.addresses.iter().any(|(n, _)| n == net))
            }
        };
        vm.plan.addresses.iter().find(|(net, _)| on(net)).map(|(_, ip)| (g, ip.to_string()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::Login;

    const SPEC: &str = "version: 1
name: shop
networks: { lab: { cidr: 10.9.0.0/24 }, back: { cidr: 10.8.0.0/24 } }
machines:
  web: { networks: { lab: 10 }, os: debian@12 }
  dc: { networks: { lab: 20 }, os: windows-server@2022, ports: { rdp: { port: 3389 } } }
  alone: { networks: { back: 30 }, os: windows-server@2022, ports: { rdp: { port: 3389 } } }
  plain: { networks: { lab: 21 }, os: windows-server@2022 }
";

    fn guest(machine: &str) -> Guest {
        Guest {
            machine: machine.to_owned(),
            login: Login::new(
                Template::from(Placeholder::Output(SnakeName::parse(&format!("address_{machine}")).unwrap())),
                "admin",
            ),
            setup_s: 60,
        }
    }

    #[test]
    fn a_windows_vm_opening_rdp_is_tunnelled_through_a_linux_one_beside_it() {
        let env = crate::testing::environment(SPEC, isoloom_spec::model::Target::CloudVm);
        let mut machines: Vec<Machine> = ["web", "dc", "alone", "plain"]
            .iter()
            .map(|n| isoloom_render::run_manifest::machine(n, isoloom_manifest::Role::Member).unwrap())
            .collect();
        let mut outputs = Vec::new();
        add(&env, &[guest("web")], &mut machines, &mut outputs).unwrap();
        let of = |n: &str| &machines.iter().find(|m| m.name.as_str() == n).unwrap().consoles[0];
        assert_eq!((of("web").protocol, of("web").note.as_deref()), (ConsoleProtocol::None, Some(LINUX)));
        let dc = of("dc");
        let Some(ConsoleSource::SshTunnel { via, user, host, port }) = &dc.source else {
            panic!("{dc:?}")
        };
        assert_eq!(
            (via.to_string(), user.to_string(), host.to_string(), *port),
            ("${outputs.address_web}".to_owned(), "admin".to_owned(), "10.9.0.20".to_owned(), 3389)
        );
        let account = dc.account.as_ref().unwrap();
        assert_eq!(
            (account.user.to_string(), account.password.to_string()),
            ("isoloom".to_owned(), "${outputs.windows_password}".to_owned())
        );
        assert_eq!(of("alone").note.as_deref(), Some(NO_JUMP));
        assert_eq!(of("plain").note.as_deref(), Some(NO_PORT));
        assert_eq!(outputs.len(), 1);
        assert!(outputs[0].sensitive && outputs[0].name.as_str() == PASSWORD_OUTPUT);
        let mut m = Module::default();
        super::outputs(&env, &mut m);
        let text = m.render().unwrap();
        assert!(text.contains("output \"windows_password\"") && text.contains("sensitive = true"), "{text}");
    }

    #[test]
    fn without_a_windows_vm_opening_rdp_there_is_no_password_output() {
        let env = crate::testing::environment(
            &SPEC.replace("ports: { rdp: { port: 3389 } }", "ports: { web: { port: 80 } }"),
            isoloom_spec::model::Target::CloudVm,
        );
        let mut m = Module::default();
        super::outputs(&env, &mut m);
        assert_eq!(m.render().unwrap(), "");
    }
}
