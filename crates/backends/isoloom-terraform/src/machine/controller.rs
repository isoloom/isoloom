//! The controller: a Debian VM on every network (at the controller address) that runs the
//! environment's playbooks over SSH with a key of its own, once every machine is set up, and the
//! checks when no Linux machine can. A target makes the VM; this module gives its key, its
//! set-up and the inventory ([`inventory`]), and its waits for the machines before the playbooks
//! ([`waits`]).

use std::net::Ipv4Addr;

use crate::hcl_builder::{Expression, Module, Template, call, reference, string};
use indexmap::IndexMap;
use isoloom_plan::addressing::names_of;
use isoloom_plan::{Controller, Environment, Step, Vm};
use isoloom_render::inventory::{CONTROLLER_INVENTORY, CONTROLLER_KEY};
use isoloom_render::{controller, steps};
use isoloom_spec::linux::CONFIG_DIR;

mod inventory;
mod waits;

pub(crate) use inventory::{WinrmAuth, cloud_inventory, inventory_template, server_access};
pub(crate) use waits::{linux_waits, windows_waits};

use super::provisioner::{Connection, Provisioner, Upload};
use super::{Shell, linux};
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

/// The controller's inventory as Isoloom's server and cloud VMs are reached.
#[cfg(test)]
mod inventory_tests {
    use isoloom_render::inventory::render;
    use isoloom_render::test_support::{INVENTORY_SPEC as SPEC, example, section};
    use isoloom_spec::model::Target;

    use isoloom_render::inventory::Access;

    use super::inventory::{WinrmAuth, cloud_access, server_access};

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
