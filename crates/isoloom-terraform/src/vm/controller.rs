//! The controller: a Debian VM on every network (at the controller address) that runs the
//! environment's playbooks over SSH with a key of its own, once every machine is set up, and the
//! checks when no Linux machine can. A target makes the VM; this module gives its key, its
//! set-up and the inventory.

use isoloom_plan::{Environment, Step};
use isoloom_render::hcl_builder::{Block, Expression, Template, call, reference, string};
use isoloom_render::inventory::{self, Access};
use isoloom_render::{address, ansible_runs, names_of, steps};

use super::provisioner::{Connection, Provisioner, Upload};
use super::{as_root, linux, res};

/// The controller's OS.
pub const OS: &str = "debian-12";
/// Where its key is uploaded before its set-up (then moved to `/etc/isoloom`).
const KEY_UPLOAD: &str = "/tmp/isoloom-controller-key";
/// Where its inventory is uploaded before its set-up.
const INVENTORY_UPLOAD: &str = "/tmp/isoloom-inventory.ini";

/// Ansible in a virtual environment (pywinrm for the Windows machines), once.
pub const ANSIBLE: &str =
    "[ -x /opt/ansible/bin/ansible-playbook ] || { python3 -m venv /opt/ansible && /opt/ansible/bin/pip install -q 'ansible-core>=2.15,<2.17' pywinrm; }";

/// Whether the environment needs a controller: its playbooks, or its checks when no Linux
/// machine can run them (a Windows-only environment).
pub fn needed(env: &Environment) -> bool {
    let linux = super::vms(env).any(|(_, mp)| !super::is_windows(mp));
    env.controller.is_some() || (!env.checks.is_empty() && !linux)
}

/// Its (CPUs, memory in MB, disk in GB if set): 1 CPU and 512 MB unless the spec says otherwise.
pub fn size(env: &Environment) -> (u32, u32, Option<u32>) {
    let s = env.controller.map(|c| c.size).unwrap_or_default();
    (s.cpus.unwrap_or(1), s.memory_mb.unwrap_or(512), s.disk_gb)
}

/// Its key: `tls_private_key.controller`.
pub fn key() -> Block {
    Block::builder("resource")
        .add_label("tls_private_key")
        .add_label("controller")
        .add_attribute(("algorithm", string("ED25519")))
        .build()
}

/// Its public key, as `authorized_keys` takes it.
pub fn public_key() -> Expression {
    call("trimspace", [reference("tls_private_key.controller.public_key_openssh")])
}

/// Run by a machine's SSH user: the controller may log in as it.
pub fn authorize() -> Template {
    Template::text("mkdir -p ~/.ssh && echo '")
        .with_expr(public_key())
        .with_text("' >> ~/.ssh/authorized_keys")
}

/// The controller's `/etc/hosts` lines: every machine at its first address.
pub fn hosts(env: &Environment) -> Option<String> {
    let spec = env.spec();
    let lines: Vec<String> = spec
        .machines
        .iter()
        .filter_map(|(o, om)| om.networks.first().map(|(n, oc)| format!("'{} {}'", address(spec, n, *oc), names_of(spec, o))))
        .collect();
    (!lines.is_empty()).then(|| format!("printf '%s\\n' {} >> /etc/hosts\n", lines.join(" ")))
}

/// The playbooks, as the controller runs them.
pub fn playbooks(env: &Environment) -> String {
    ansible_runs(env.spec(), &[])
}

/// The Terraform expression of the Windows machines' password, as the cloud modules write it.
const WINDOWS_PASSWORD: &str = "random_password.windows.result";

/// A cloud's inventory (the shared one, `isoloom_render::inventory`): each Linux VM as its
/// image's user (`user(os)`), the Windows VMs over WinRM with the generated password, which is
/// interpolated where the inventory names it.
pub fn cloud_inventory(env: &Environment, user: impl Fn(&str) -> String) -> Template {
    let marker = format!("${{{WINDOWS_PASSWORD}}}");
    let text = inventory::render(env, &Access::cloud(user, &marker));
    let mut t = Template::new();
    for (i, part) in text.split(marker.as_str()).enumerate() {
        if i > 0 {
            t.push_expr(reference(WINDOWS_PASSWORD));
        }
        t.push_text(part);
    }
    t
}

/// The controller over SSH, as Terraform reaches it.
#[derive(Debug, Clone)]
pub struct Ssh<'a> {
    /// The plan.
    pub env: &'a Environment,
    /// What replaces its set-up (the instance's id).
    pub replace_on: Expression,
    /// Its address.
    pub host: Expression,
    /// Its extra interfaces, found by MAC (as a machine's `at_boot`).
    pub at_boot: Vec<Template>,
    /// The inventory.
    pub inventory: Template,
}

/// Its set-up over SSH (`terraform_data.isoloom_controller`), once every VM is set up: the
/// project, its key and the inventory uploaded, Ansible installed, the playbooks run.
pub fn provisioner(c: Ssh) -> Provisioner {
    let env = c.env;
    let mut cmds: Vec<Template> = vec!["set -e".into(), linux::CLOUD_INIT_WAIT.into()];
    cmds.extend(c.at_boot);
    let root = |s: Option<String>| s.map(|s| Template::from(as_root(&s, true)));
    cmds.extend(root(steps::linux(env, "", &Step::Prelude)));
    cmds.extend(root(steps::linux(env, "", &Step::Project)));
    cmds.extend(root(hosts(env)));
    let install = format!(
        "mkdir -p /etc/isoloom && install -m 0600 {KEY_UPLOAD} /etc/isoloom/id_ed25519 && install -m 0644 {INVENTORY_UPLOAD} /etc/isoloom/inventory.ini && rm -f {KEY_UPLOAD} {INVENTORY_UPLOAD}\n"
    );
    let packages = "DEBIAN_FRONTEND=noninteractive apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq python3-venv curl netcat-openbsd >/dev/null\n";
    for script in [install.as_str(), packages, ANSIBLE, &playbooks(env)] {
        cmds.push(as_root(script, true).into());
    }
    cmds.extend(root(steps::linux(env, "", &Step::Ready)));
    Provisioner {
        name: "isoloom_controller".into(),
        replace_on: c.replace_on,
        connection: Connection::Ssh {
            host: c.host,
            user: "admin".into(),
        },
        uploads: vec![
            Upload::Project {
                id: "controller".into(),
                destination: steps::PROJECT_ARCHIVE,
            },
            Upload::Content {
                content: reference("tls_private_key.controller.private_key_openssh"),
                destination: KEY_UPLOAD.into(),
            },
            Upload::Content {
                content: c.inventory.into_expression(),
                destination: INVENTORY_UPLOAD.into(),
            },
        ],
        parts: vec![cmds.into_iter().map(Template::into_expression).collect()],
        depends_on: super::vms(env).map(|(n, _)| reference(&format!("terraform_data.{}", res(n)))).collect(),
    }
}
