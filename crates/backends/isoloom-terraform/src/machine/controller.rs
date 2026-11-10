//! The controller: a Debian VM on every network (at the controller address) that runs the
//! environment's playbooks over SSH with a key of its own, once every machine is set up, and the
//! checks when no Linux machine can. A target makes the VM; this module gives its key, its
//! set-up and the inventory.

use std::net::Ipv4Addr;

use crate::hcl_builder::{Expression, Module, Template, call, reference, string};
use indexmap::IndexMap;
use isoloom_plan::addressing::names_of;
use isoloom_plan::{Controller, Environment, Step, Vm};
use isoloom_render::inventory::{self, Access, CONTROLLER_INVENTORY, CONTROLLER_KEY, Vars, vars};
use isoloom_render::{controller, steps};
use isoloom_spec::linux::CONFIG_DIR;
use isoloom_spec::model::Winrm;

use super::provisioner::{Connection, Provisioner, Upload};
use super::{Shell, linux, windows};
use crate::module::{ident, resource};

/// The controller's OS.
pub(crate) const OS: &str = "debian-12";

/// Its name: its host name, and its machine in a run manifest.
pub(crate) const NAME: &str = isoloom_manifest::contract::CONTROLLER;
/// Where its key is uploaded before its set-up (then moved to `/etc/isoloom`).
const KEY_UPLOAD: &str = "/tmp/isoloom-controller-key";
/// Where its inventory is uploaded before its set-up.
const INVENTORY_UPLOAD: &str = "/tmp/isoloom-inventory.ini";

/// The mode of its key and its inventory: owner-only (root's), as the inventory holds the Windows
/// machines' password and only root runs the playbooks and checks that read it.
pub(crate) const OWNER_ONLY: &str = "0600";

/// Whether the environment needs a controller: its playbooks, or its checks when no Linux
/// machine can run them (a Windows-only environment).
pub(crate) fn needed(env: &Environment) -> bool {
    let linux = env.vms().any(|vm| !vm.plan.is_windows());
    env.controller.is_some() || (!env.checks.is_empty() && !linux)
}

/// The Debian packages it needs before Ansible: a virtual environment for it, `curl` and `nc` for
/// its waits, the SSH client for the playbooks.
pub(crate) const PACKAGES: [&str; 4] = ["python3-venv", "curl", "netcat-openbsd", "openssh-client"];

/// [`PACKAGES`] installed with apt, as a root script.
pub(crate) fn install_packages() -> String {
    format!(
        "DEBIAN_FRONTEND=noninteractive apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq {} >/dev/null\n",
        PACKAGES.join(" ")
    )
}

/// Its disk in GB when the spec leaves it out, on every target: Debian, Python, Ansible and the
/// project fit, with room for their logs.
pub(crate) const DISK_GB: u32 = 10;

/// Its (CPUs, memory in MB, disk in GB): 1 CPU, 512 MB and [`DISK_GB`] unless the spec says
/// otherwise.
pub(crate) fn size(env: &Environment) -> (u32, u32, u32) {
    env.controller.map_or((Controller::CPUS, Controller::MEMORY_MB, DISK_GB), |c| {
        (c.cpus, c.memory_mb, c.disk_gb.unwrap_or(DISK_GB))
    })
}

/// Its key, `tls_private_key.controller`, when the environment needs a controller.
pub(crate) fn write_key(env: &Environment, m: &mut Module) {
    if needed(env) {
        m.commented(
            "The controller's own key: it runs the playbooks over SSH on every machine.",
            resource("tls_private_key", "controller")
                .add_attribute(("algorithm", string("ED25519")))
                .build(),
        );
    }
}

/// The controller's address on each network.
pub(crate) fn addresses(env: &Environment) -> IndexMap<String, Ipv4Addr> {
    env.networks.iter().map(|(n, p)| (n.clone(), p.cidr.controller())).collect()
}

/// Its key's resource ([`write_key`]).
const KEY: &str = "tls_private_key.controller";

/// Its public key, as `authorized_keys` takes it.
pub(crate) fn public_key() -> Expression {
    call("trimspace", [reference(&format!("{KEY}.public_key_openssh"))])
}

/// Its private key, OpenSSH's format, as it is written onto the controller.
pub(crate) fn private_key() -> Expression {
    reference(&format!("{KEY}.private_key_openssh"))
}

/// Run by a machine's SSH user: the controller may log in as it.
pub(crate) fn authorize() -> Template {
    Template::text("mkdir -p ~/.ssh && echo '")
        .with_expr(public_key())
        .with_text("' >> ~/.ssh/authorized_keys")
}

/// Run as root by a machine's own set-up: the controller may log in as `user` (made by the
/// cloud's first boot, waited for).
pub(crate) fn authorize_as(user: &str) -> Template {
    Template::text(format!(
        "u='{user}'; i=0; until id \"$u\" >/dev/null 2>&1; do i=$((i+2)); [ $i -lt 600 ] || exit 1; sleep 2; done; h=$(getent passwd \"$u\" | cut -d: -f6); install -d -m 700 -o \"$u\" -g \"$(id -gn \"$u\")\" \"$h/.ssh\" && echo '"
    ))
    .with_expr(public_key())
    .with_text("' >> \"$h/.ssh/authorized_keys\" && chown \"$u\" \"$h/.ssh/authorized_keys\" && chmod 600 \"$h/.ssh/authorized_keys\"")
}

/// The controller's `/etc/hosts` lines: every machine at its first address.
pub(crate) fn hosts(env: &Environment) -> Option<String> {
    let spec = env.spec();
    let lines: Vec<String> = env
        .machines
        .iter()
        .filter_map(|(o, mp)| mp.addresses.values().next().map(|a| format!("'{a} {}'", names_of(spec, o))))
        .collect();
    (!lines.is_empty()).then(|| format!("printf '%s\\n' {} >> /etc/hosts\n", lines.join(" ")))
}

/// The playbooks, as the controller runs them.
pub(crate) fn playbooks(env: &Environment) -> String {
    controller::provision_script(env.spec(), &[])
}

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

/// The controller over SSH, as Terraform reaches it.
#[derive(Debug, Clone)]
pub(crate) struct Ssh<'a> {
    /// The plan.
    pub(crate) env: &'a Environment,
    /// What replaces its set-up (the instance's id).
    pub(crate) replace_on: Expression,
    /// Its address.
    pub(crate) host: Expression,
    /// The SSH user (it has `sudo`).
    pub(crate) user: String,
    /// Its extra interfaces, found by MAC (as a machine's `at_boot`).
    pub(crate) at_boot: Vec<Template>,
    /// The inventory.
    pub(crate) inventory: Template,
}

/// Its set-up over SSH (`terraform_data.isoloom_controller`), once every VM is set up: the
/// project, its key and the inventory uploaded, the spec's certificate authorities trusted,
/// Ansible installed, `waits` run (as root) before
/// the playbooks (what the machines Terraform doesn't set up need: a Windows machine's ready
/// marker, over WinRM), the playbooks run (ESXi).
pub(crate) fn provisioner_waiting(c: Ssh, waits: &[String]) -> Provisioner {
    let env = c.env;
    let mut cmds: Vec<Template> = vec!["set -e".into(), linux::CLOUD_INIT_WAIT.into()];
    cmds.extend(c.at_boot);
    let root = |s: Option<String>| s.map(|s| Template::from(Shell::Sudo.command(&s)));
    cmds.extend(root(steps::linux(env, "", &Step::Prelude)));
    cmds.extend(root(steps::linux(env, "", &Step::Project)));
    cmds.extend(root(steps::trust_authorities(env)));
    cmds.extend(root(hosts(env)));
    let install = format!(
        "mkdir -p {CONFIG_DIR} && install -m {OWNER_ONLY} {KEY_UPLOAD} {CONTROLLER_KEY} && install -m {OWNER_ONLY} {INVENTORY_UPLOAD} {CONTROLLER_INVENTORY} && rm -f {KEY_UPLOAD} {INVENTORY_UPLOAD}\n"
    );
    let packages = install_packages();
    for script in [install.as_str(), packages.as_str(), isoloom_render::controller::INSTALL_ANSIBLE] {
        cmds.push(Shell::Sudo.command(script).into());
    }
    cmds.extend(waits.iter().map(|w| Template::from(Shell::Sudo.command(w))));
    cmds.push(Shell::Sudo.command(&playbooks(env)).into());
    cmds.extend(root(steps::linux(env, "", &Step::Ready)));
    Provisioner {
        name: "isoloom_controller".to_owned(),
        replace_on: c.replace_on,
        connection: Connection::Ssh { host: c.host, user: c.user },
        uploads: vec![
            Upload::Project {
                id: "isoloom_controller".to_owned(),
                destination: steps::PROJECT_ARCHIVE,
            },
            Upload::Content {
                content: private_key(),
                destination: KEY_UPLOAD.to_owned(),
            },
            Upload::Content {
                content: c.inventory.into_expression(),
                destination: INVENTORY_UPLOAD.to_owned(),
            },
        ],
        parts: vec![cmds.into_iter().map(Template::into_expression).collect()],
        depends_on: env.vms().map(|Vm { name: n, .. }| reference(&format!("terraform_data.{}", ident(n)))).collect(),
    }
}

/// Each Linux machine's own set-up must be done (its ready file says `ready`) before the
/// playbooks run: its name, and the wait, over SSH with the controller's key as `user` of its OS at the
/// machine's first address.
pub(crate) fn linux_waits(env: &Environment, user: &dyn Fn(&str) -> String) -> Vec<(String, String)> {
    env.vms()
        .filter(|vm| !vm.plan.is_windows())
        .filter_map(|Vm { name, plan: mp, os }| {
            let a = mp.addresses.values().next()?;
            let u = user(os);
            let ready = isoloom_render::steps::READY_FILE;
            // Its ready file's first line: `ready` goes on, `failed: <step>` stops the playbooks.
            let (done, failed) = (isoloom_manifest::ReadyLine::READY, isoloom_render::steps::failed_pattern());
            let wait = format!(
                "i=0; until s=$(ssh -o StrictHostKeyChecking=no -o ConnectTimeout=5 -i {CONTROLLER_KEY} {u}@{a} head -n 1 {ready} 2>/dev/null) && [ \"$s\" = {done} ]; do case \"$s\" in {failed}) echo \"{name}: $s\"; exit 1;; esac; i=$((i+1)); [ $i -gt 240 ] && {{ echo '{name} not ready'; exit 1; }}; sleep 5; done"
            );
            Some((name.to_owned(), Shell::Root.command(&wait)))
        })
        .collect()
}

/// The controller's wait for each Windows machine's ready marker, through Ansible over WinRM
/// (the inventory's own connection), before the playbooks run: a Windows machine set up at its
/// first boot (cloudbase-init) has no set-up of Terraform's to wait for.
pub(crate) fn windows_waits(env: &Environment) -> Vec<String> {
    env.vms()
        .filter(|vm| vm.plan.is_windows())
        .map(|Vm { name, .. }| {
            format!(
                "i=0; until /opt/ansible/bin/ansible -i {CONTROLLER_INVENTORY} {name} -m raw -a 'if (-not (Test-Path {ready})) {{ exit 1 }}' >/dev/null 2>&1; do i=$((i+1)); [ $i -gt 360 ] && {{ echo '{name} not ready'; exit 1; }}; sleep 10; done",
                ready = isoloom_spec::windows::READY_FILE
            )
        })
        .collect()
}

/// The controller's inventory as Isoloom's server and cloud VMs are reached.
#[cfg(test)]
mod inventory_tests {
    use isoloom_render::inventory::render;
    use isoloom_render::test_support::{INVENTORY_SPEC as SPEC, example, section};
    use isoloom_spec::model::Target;

    use super::{Access, WinrmAuth, cloud_access, server_access};

    /// AWS's image users, as the cloud-vm module names them.
    fn aws() -> Access<'static> {
        let user = |os: &str| if os.starts_with("ubuntu") { "ubuntu" } else { "admin" }.to_owned();
        cloud_access(user, "${random_password.windows.result}", WinrmAuth::Basic)
    }

    #[test]
    fn a_server_keeps_the_groups_and_gives_windows_its_winrm_account() {
        let inv = render(&crate::testing::environment(SPEC, Target::Proxmox), &server_access("pw"));
        assert!(section(&inv, "linux").starts_with("web ansible_host=10.8.0.10 ansible_user=isoloom code="));
        assert_eq!(section(&inv, "windows"), "dc ansible_host=10.9.0.20\n\n");
        assert!(
            section(&inv, "windows:vars").starts_with("ansible_user=isoloom\nansible_password=pw\nansible_connection=winrm\n"),
            "{inv}"
        );
        assert_eq!(section(&inv, "servers"), "web\ndc\n");
    }

    #[test]
    fn a_cloud_names_each_linux_user_and_the_windows_password() {
        let access = cloud_access(
            |os| if os == "debian-12" { "admin".into() } else { "ubuntu".into() },
            "${random_password.windows.result}",
            WinrmAuth::Basic,
        );
        let inv = render(&crate::testing::environment(SPEC, Target::CloudVm), &access);
        assert!(section(&inv, "linux").starts_with("web ansible_host=10.8.0.10 ansible_user=admin code="));
        assert!(section(&inv, "windows:vars").contains("ansible_password=${random_password.windows.result}\n"));
        assert!(section(&inv, "linux:vars").starts_with("ansible_ssh_private_key_file=/etc/isoloom/id_ed25519\n"));
    }

    #[test]
    fn proxmox_ansible_pair_keeps_the_groups() {
        insta::assert_snapshot!(render(
            &example("ansible-pair", Target::Proxmox).unwrap(),
            &server_access("${random_password.windows.result}")
        ));
    }

    #[test]
    fn aws_ansible_pair() {
        insta::assert_snapshot!(render(&example("ansible-pair", Target::CloudVm).unwrap(), &aws()));
    }

    #[test]
    fn aws_corp_ad_basics_windows() {
        insta::assert_snapshot!(render(&example("corp-ad-basics", Target::CloudVm).unwrap(), &aws()));
    }

    #[test]
    fn azure_windows_hosts_authenticate_with_ntlm() {
        let azure = cloud_access(|_| "isoloom".to_owned(), "${random_password.windows.result}", WinrmAuth::Ntlm);
        let inv = render(&example("corp-ad-basics", Target::CloudVm).unwrap(), &azure);
        assert!(inv.contains("ansible_winrm_transport=ntlm\n"), "{inv}");
        assert!(!inv.contains("ansible_winrm_transport=basic"), "{inv}");
    }

    #[test]
    fn server_windows_hosts_get_the_isoloom_account_over_winrm() {
        let inv = render(
            &example("windows-hello", Target::Proxmox).unwrap(),
            &server_access("${random_password.windows.result}"),
        );
        for v in [
            "ansible_user=isoloom",
            "ansible_password=${random_password.windows.result}",
            "ansible_connection=winrm",
        ] {
            assert!(inv.contains(&format!("{v}\n")), "{v}: {inv}");
        }
    }

    #[test]
    fn rendering_twice_gives_the_same_inventory() {
        for (name, target) in [("ansible-pair", Target::Vagrant), ("corp-ad-basics", Target::CloudVm)] {
            let once = render(&example(name, target).unwrap(), &aws());
            assert_eq!(once, render(&example(name, target).unwrap(), &aws()), "{name}");
        }
    }
}
