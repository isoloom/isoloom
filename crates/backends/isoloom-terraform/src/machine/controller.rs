//! The controller: a Debian VM on every network (at the controller address) that runs the
//! environment's playbooks over SSH with a key of its own, once every machine is set up, and the
//! checks when no Linux machine can. A target makes the VM; this module gives its key, its
//! set-up and the inventory.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_plan::{Environment, Step};
use isoloom_render::hcl_builder::{Expression, Module, Template, call, reference, string};
use isoloom_render::inventory::{self, Access, WinrmAuth};
use isoloom_render::{address, ansible_runs, names_of, steps};
use isoloom_spec::validate::Cidr;

use super::provisioner::{Connection, Provisioner, Upload};
use super::{Shell, linux, vms, windows};
use crate::module::{ident, resource};

/// The controller's OS.
pub(crate) const OS: &str = "debian-12";
/// Where its key is uploaded before its set-up (then moved to `/etc/isoloom`).
const KEY_UPLOAD: &str = "/tmp/isoloom-controller-key";
/// Where its inventory is uploaded before its set-up.
const INVENTORY_UPLOAD: &str = "/tmp/isoloom-inventory.ini";

/// Ansible in a virtual environment (pywinrm for the Windows machines), once.
pub(crate) const ANSIBLE: &str =
    "[ -x /opt/ansible/bin/ansible-playbook ] || { python3 -m venv /opt/ansible && /opt/ansible/bin/pip install -q 'ansible-core>=2.15,<2.17' pywinrm; }";

/// Whether the environment needs a controller: its playbooks, or its checks when no Linux
/// machine can run them (a Windows-only environment).
pub(crate) fn needed(env: &Environment) -> bool {
    let linux = vms(env).any(|(_, mp)| !mp.is_windows());
    env.controller.is_some() || (!env.checks.is_empty() && !linux)
}

/// Its (CPUs, memory in MB, disk in GB if set): 1 CPU and 512 MB unless the spec says otherwise.
pub(crate) fn size(env: &Environment) -> (u32, u32, Option<u32>) {
    let s = env.controller.map(|c| c.size).unwrap_or_default();
    (s.cpus.unwrap_or(1), s.memory_mb.unwrap_or(512), s.disk_gb)
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
    env.networks
        .iter()
        .map(|(n, p)| (n.clone(), Cidr::parse(&p.cidr).map_or(Ipv4Addr::UNSPECIFIED, Cidr::controller)))
        .collect()
}

/// Its public key, as `authorized_keys` takes it.
pub(crate) fn public_key() -> Expression {
    call("trimspace", [reference("tls_private_key.controller.public_key_openssh")])
}

/// Run by a machine's SSH user: the controller may log in as it.
pub(crate) fn authorize() -> Template {
    Template::text("mkdir -p ~/.ssh && echo '")
        .with_expr(public_key())
        .with_text("' >> ~/.ssh/authorized_keys")
}

/// The controller's `/etc/hosts` lines: every machine at its first address.
pub(crate) fn hosts(env: &Environment) -> Option<String> {
    let spec = env.spec();
    let lines: Vec<String> = spec
        .machines
        .iter()
        .filter_map(|(o, om)| om.networks.first().map(|(n, oc)| format!("'{} {}'", address(spec, n, *oc), names_of(spec, o))))
        .collect();
    (!lines.is_empty()).then(|| format!("printf '%s\\n' {} >> /etc/hosts\n", lines.join(" ")))
}

/// The playbooks, as the controller runs them.
pub(crate) fn playbooks(env: &Environment) -> String {
    ansible_runs(env.spec(), &[])
}

/// A cloud's inventory (the shared one, `isoloom_render::inventory`): each Linux VM as its
/// image's user (`user(os)`), the Windows VMs over WinRM (authenticated with `auth`, as
/// Terraform's own connection is) with the generated password.
pub(crate) fn cloud_inventory(env: &Environment, user: impl Fn(&str) -> String, auth: WinrmAuth) -> Template {
    inventory_template(env, |password| Access::cloud(user, password, auth))
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
/// project, its key and the inventory uploaded, Ansible installed, the playbooks run.
pub(crate) fn provisioner(c: Ssh) -> Provisioner {
    provisioner_waiting(c, &[])
}

/// [`provisioner`], with `waits` run (as root) once Ansible is installed and before the
/// playbooks: what the machines Terraform doesn't set up need (a Windows machine's ready
/// marker, over WinRM).
pub(crate) fn provisioner_waiting(c: Ssh, waits: &[String]) -> Provisioner {
    let env = c.env;
    let mut cmds: Vec<Template> = vec!["set -e".into(), linux::CLOUD_INIT_WAIT.into()];
    cmds.extend(c.at_boot);
    let root = |s: Option<String>| s.map(|s| Template::from(Shell::Sudo.command(&s)));
    cmds.extend(root(steps::linux(env, "", &Step::Prelude)));
    cmds.extend(root(steps::linux(env, "", &Step::Project)));
    cmds.extend(root(hosts(env)));
    let install = format!(
        "mkdir -p /etc/isoloom && install -m 0600 {KEY_UPLOAD} /etc/isoloom/id_ed25519 && install -m 0644 {INVENTORY_UPLOAD} /etc/isoloom/inventory.ini && rm -f {KEY_UPLOAD} {INVENTORY_UPLOAD}\n"
    );
    let packages = "DEBIAN_FRONTEND=noninteractive apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq python3-venv curl netcat-openbsd >/dev/null\n";
    for script in [install.as_str(), packages, ANSIBLE] {
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
                id: "controller".to_owned(),
                destination: steps::PROJECT_ARCHIVE,
            },
            Upload::Content {
                content: reference("tls_private_key.controller.private_key_openssh"),
                destination: KEY_UPLOAD.to_owned(),
            },
            Upload::Content {
                content: c.inventory.into_expression(),
                destination: INVENTORY_UPLOAD.to_owned(),
            },
        ],
        parts: vec![cmds.into_iter().map(Template::into_expression).collect()],
        depends_on: vms(env).map(|(n, _)| reference(&format!("terraform_data.{}", ident(n)))).collect(),
        announce: None,
    }
}

/// The controller's wait for each Windows machine's ready marker, through Ansible over WinRM
/// (the inventory's own connection), before the playbooks run: a Windows machine set up at its
/// first boot (cloudbase-init) has no set-up of Terraform's to wait for.
pub(crate) fn windows_waits(env: &Environment) -> Vec<String> {
    vms(env)
        .filter(|(_, mp)| mp.is_windows())
        .map(|(name, _)| {
            format!(
                "i=0; until /opt/ansible/bin/ansible -i /etc/isoloom/inventory.ini {name} -m raw -a 'if (-not (Test-Path C:\\isoloom\\ready)) {{ exit 1 }}' >/dev/null 2>&1; do i=$((i+1)); [ $i -gt 360 ] && {{ echo '{name} not ready'; exit 1; }}; sleep 10; done"
            )
        })
        .collect()
}
