//! The step kinds that run or guard something: `service` (with the restart handler the recipe's
//! earlier changes notify), `command` behind its guard, `firewall` through `ansible.posix` or a
//! guarded `command`, and `run`, `reboot` and `playbook`. Each matches the shell library's guard
//! and state (`isoloom-render::configure::shell`), so a second apply changes nothing.

use isoloom_plan::configure::{CommandPlan, Family, PortSpec, RunPlan, ServicePlan, ServiceSource};
use isoloom_render::configure::{Configured, linux_unit};
use isoloom_spec::syntax::{Guard, Invocation};
use serde_json::{Value, json};

use super::task::Task;
use super::tasks::project;

/// The `service` step: an enabled unit started (a defined one's file written first), or a disabled
/// one stopped. Restarts come from the recipe's earlier changes notifying the handler and the
/// `meta: flush_handlers` the role ends with, so the restart rule is the shell library's.
pub(super) fn service(c: &Configured<'_>, s: &ServicePlan) -> Vec<Value> {
    let mut out = Vec::new();
    let family = c.recipe().family;
    if let ServiceSource::Defined { run, user, cwd } = &s.source {
        let unit = linux_unit(family, &s.unit, run, user.as_ref(), cwd.as_ref());
        let (dest, mode) = if family == Family::Alpine {
            (format!("/etc/init.d/{}", s.unit), "0755")
        } else {
            (format!("/etc/systemd/system/{}.service", s.unit), "0644")
        };
        out.push(
            Task::new(format!("the unit {}", s.unit))
                .module("ansible.builtin.copy", json!({ "content": unit, "dest": dest, "mode": mode }))
                .notify(&[s.unit.to_string()])
                .into_value(),
        );
    }
    let reload = matches!(s.source, ServiceSource::Defined { .. }) && family != Family::Alpine;
    let (state, enabled) = if s.enabled { ("started", true) } else { ("stopped", false) };
    let mut args = serde_json::Map::new();
    args.insert("name".to_owned(), Value::String(s.unit.to_string()));
    args.insert("enabled".to_owned(), Value::Bool(enabled));
    args.insert("state".to_owned(), Value::String(state.to_owned()));
    let module = if reload {
        args.insert("daemon_reload".to_owned(), Value::Bool(true));
        "ansible.builtin.systemd_service"
    } else {
        "ansible.builtin.service"
    };
    out.push(Task::new(format!("{} {state}", s.unit)).module(module, Value::Object(args)).into_value());
    out
}

/// The `command` step: its program and arguments behind its guard. `creates` is the module's own;
/// `unless` is a check task (which changes nothing) whose result the command's `when` reads. It
/// reports `changed` whenever it runs, so an earlier change's restart rule and `ansible-lint`'s
/// `no-changed-when` are both satisfied. `vars` prefixes its registered guard.
pub(super) fn command(cmd: &CommandPlan, notify: &[String], vars: &str, seq: usize) -> Vec<Value> {
    let (module, run) = invocation(&cmd.run);
    let mut out = Vec::new();
    // Without `creates`, the command runs every apply, so it reports `changed` when it runs
    // (and `ansible-lint`'s `no-changed-when` is satisfied); with `creates`, the module itself
    // reports `changed` only when it actually ran, so `changed_when` would wrongly force it.
    let mut task = Task::new(format!("command {}", summary(&cmd.run)))
        .module(module, run.clone())
        .set("changed_when", Value::Bool(true));
    match &cmd.guard {
        Guard::Creates(path) => {
            let mut args = run.as_object().cloned().unwrap_or_default();
            args.insert("creates".to_owned(), Value::String(path.to_string()));
            task = Task::new(format!("command {}", summary(&cmd.run))).module(module, Value::Object(args));
        }
        Guard::Unless(check) => {
            let (gmod, grun) = invocation(check);
            let reg = format!("{vars}_guard_{seq}");
            out.push(
                Task::new(format!("the guard of command {}", summary(&cmd.run)))
                    .module(gmod, grun)
                    .set("register", Value::String(reg.clone()))
                    .set("changed_when", Value::Bool(false))
                    .set("failed_when", Value::Bool(false))
                    .into_value(),
            );
            task = task.set("when", Value::String(format!("{reg}.rc != 0")));
        }
        _ => {}
    }
    out.push(task.notify(notify).into_value());
    out
}

/// The module and arguments that run an invocation: a text through the `shell` module (as `sh -c`
/// does), a program and its arguments through `command` (run as they are).
fn invocation(i: &Invocation) -> (&'static str, Value) {
    match i {
        Invocation::Shell(text) => ("ansible.builtin.shell", json!({ "cmd": text })),
        Invocation::Exec(argv) => (
            "ansible.builtin.command",
            json!({ "argv": argv.args().iter().map(|a| Value::String(a.clone())).collect::<Vec<_>>() }),
        ),
        _ => ("ansible.builtin.command", json!({ "argv": ["false"] })),
    }
}

/// A short name for a command, for a task's `name`.
fn summary(i: &Invocation) -> String {
    match i {
        Invocation::Shell(text) => text.lines().next().unwrap_or_default().to_owned(),
        Invocation::Exec(argv) => argv.program().to_owned(),
        _ => "false".to_owned(),
    }
}

/// The `firewall` step: each port opened in the OS's firewall when one runs (firewalld through
/// `ansible.posix`, else a guarded `ufw` command, since `community.general.ufw` isn't allowed),
/// and nothing where no firewall runs, as the shell library leaves it.
pub(super) fn firewall(ports: &[PortSpec], notify: &[String], vars: &str, seq: usize) -> Vec<Value> {
    if ports.is_empty() {
        return Vec::new();
    }
    let fw = format!("{vars}_firewalld_{seq}");
    let uf = format!("{vars}_ufw_{seq}");
    let added = format!("{vars}_ufw_add_{seq}");
    let mut out = vec![detect("firewall-cmd --state", &fw), detect("ufw status", &uf)];
    for p in ports {
        let spec = format!("{}/{}", p.port, if p.protocol.is_tcp() { "tcp" } else { "udp" });
        out.push(
            Task::new(format!("firewalld allows {spec}"))
                .module(
                    "ansible.posix.firewalld",
                    json!({ "port": spec, "permanent": true, "immediate": true, "state": "enabled" }),
                )
                .set("when", Value::String(format!("{fw}.rc == 0")))
                .notify(notify)
                .into_value(),
        );
        out.push(
            Task::new(format!("ufw allows {spec}"))
                .module("ansible.builtin.command", json!({ "argv": ["ufw", "allow", spec] }))
                .set("when", Value::String(format!("'Status: active' in {uf}.stdout")))
                .set(
                    "changed_when",
                    Value::String(format!("'Rule added' in {added}.stdout or 'Rules updated' in {added}.stdout")),
                )
                .set("register", Value::String(added.clone()))
                .notify(notify)
                .into_value(),
        );
    }
    out
}

/// A command that checks whether a firewall runs, registered for the port tasks' `when`.
fn detect(cmd: &str, register: &str) -> Value {
    Task::new(format!("is {} running", cmd.split(' ').next().unwrap_or(cmd)))
        .module("ansible.builtin.command", json!({ "cmd": cmd }))
        .set("register", Value::String(register.to_owned()))
        .set("changed_when", Value::Bool(false))
        .set("failed_when", Value::Bool(false))
        .into_value()
}

/// The `run` step: a project script run behind its `creates`.
pub(super) fn run(r: &RunPlan, notify: &[String]) -> Value {
    let mut args = serde_json::Map::new();
    args.insert("cmd".to_owned(), Value::String(project(r.script.as_str())));
    let task = Task::new(format!("run {}", r.script));
    let task = match &r.creates {
        Some(path) => {
            args.insert("creates".to_owned(), Value::String(path.to_string()));
            task.module("ansible.builtin.script", Value::Object(args))
        }
        // No `creates`: it runs every apply (the shell renderer runs it too), so it always
        // reports `changed` (`ansible-lint`'s `no-changed-when`).
        None => task
            .module("ansible.builtin.script", Value::Object(args))
            .set("changed_when", Value::Bool(true)),
    };
    task.notify(notify).into_value()
}

/// The `reboot` step.
pub(super) fn reboot() -> Value {
    Task::new("reboot").module("ansible.builtin.reboot", json!({})).into_value()
}

/// The `playbook` step: the project's playbook run on the host itself (`-c local`), as the shell
/// carrier runs it.
pub(super) fn playbook(p: &isoloom_spec::syntax::ProjectPath, notify: &[String]) -> Value {
    Task::new(format!("playbook {p}"))
        .module(
            "ansible.builtin.command",
            json!({ "argv": ["ansible-playbook", "-c", "local", "-i", "localhost,", project(p.as_str())] }),
        )
        .set("changed_when", Value::Bool(true))
        .notify(notify)
        .into_value()
}
