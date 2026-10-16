//! The controller on a cloud, set up from its own user data (`crate::machine::user_data`), as on
//! Proxmox: the project the runner delivers, its key and the inventory written from its user
//! data, the hosts file, Ansible, then a wait for each machine's own set-up (its ready file over
//! SSH, a Windows machine's marker over WinRM) before the playbooks run.

use isoloom_plan::{Environment, MachinePlan, Step};
use isoloom_render::hcl_builder::{Expression, Template, call, reference};
use isoloom_render::steps;

use crate::machine::user_data::{SetUp, Tracked, expiry_shutdown, project_wait};
use crate::machine::{Shell, controller};

/// The packages the controller needs before Ansible.
const PACKAGES: &str = "DEBIAN_FRONTEND=noninteractive apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq python3-venv curl netcat-openbsd openssh-client >/dev/null";

/// `content` written owner-only at `path`, base64 on its way (a key, the inventory).
fn written(path: &str, mode: &str, content: Expression) -> Template {
    Template::text("mkdir -p /etc/isoloom && (umask 077 && echo ")
        .with_expr(call("base64encode", [content]))
        .with_text(format!(" | base64 -d > {path}) && chmod {mode} {path}"))
}

/// The controller's set-up: `at_boot` first (its interfaces after the first), then the project,
/// its key and `inventory`, the hosts file, Ansible, each machine's set-up awaited (as
/// `user(mp)` over SSH), the playbooks, the health check.
pub(crate) fn set_up(env: &Environment, user: &dyn Fn(&MachinePlan) -> String, inventory: Template, at_boot: &[Template]) -> SetUp {
    let mut part: Vec<Tracked> = at_boot.iter().map(|c| Tracked::new("the controller's interfaces", c.clone())).collect();
    part.push(project_wait());
    part.push(Tracked::new(
        "the controller's key",
        written("/etc/isoloom/id_ed25519", "0600", reference("tls_private_key.controller.private_key_openssh")),
    ));
    part.push(Tracked::new(
        "the inventory",
        written("/etc/isoloom/inventory.ini", "0644", inventory.into_expression()),
    ));
    let root = |s: Option<String>| s.map(|s| Shell::Root.command(&s));
    if let Some(prelude) = root(steps::linux(env, "", &Step::Prelude)) {
        part.push(Tracked::new("system constraints", prelude));
    }
    if let Some(hosts) = root(controller::hosts(env)) {
        part.push(Tracked::new("hosts file", hosts));
    }
    part.push(Tracked::new("packages", Shell::Root.command(PACKAGES)));
    part.push(Tracked::new("installing Ansible", Shell::Root.command(controller::ANSIBLE)));
    for (name, wait) in controller::linux_waits(env, user) {
        part.push(Tracked::new(format!("waiting for {name}"), wait));
    }
    for wait in controller::windows_waits(env) {
        part.push(Tracked::new("waiting for the Windows machines", Shell::Root.command(&wait)));
    }
    part.push(Tracked::new("the playbooks", Shell::Root.command(&controller::playbooks(env))));
    if let Some(ready) = root(steps::linux(env, "", &Step::Ready)) {
        part.push(Tracked::new("health check", ready));
    }
    SetUp {
        each_boot: vec![expiry_shutdown()],
        parts: vec![part],
        again: at_boot.to_vec(),
    }
}
