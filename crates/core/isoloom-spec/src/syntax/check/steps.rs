//! A step's checks that need its machine: the kind of file each script is for the OS, what only
//! apt's repositories have, the names a Linux account may take, and the ports a firewall step
//! opens. What a target can't run yet is no spec error: the plan refuses it on that target.

use super::super::machine::Machine;
use super::super::problem::{Code, FieldPath, Problem, Problems};
use super::super::step::{FirewallPort, Step, has_extension};
use super::port_reference;

/// Step `step` of machine `m`, at `at` (`machines.web.app.steps[2]`).
pub(super) fn check(at: &FieldPath, m: &Machine, step: &Step, out: &mut Problems) {
    let windows = m.os.is_some_and(super::super::os::OsRef::is_windows);
    if windows {
        windows_step(at, step, out);
    } else {
        linux_step(at, m, step, out);
    }
    if let Step::Firewall(f) = step {
        for (i, port) in f.allow.iter().enumerate() {
            if let FirewallPort::Named(name) = port
                && !m.ports.contains_key(name)
            {
                out.push(port_reference(&at.key("firewall").key("allow").index(i), name.as_str(), m));
            }
        }
    }
}

/// A Windows machine's step: scripts are PowerShell, and it restarts from its own script.
fn windows_step(at: &FieldPath, step: &Step, out: &mut Problems) {
    match step {
        Step::Run { script, .. } if !has_extension(script.as_str(), "ps1") => {
            out.push(Problem::new(
                Code::InvalidValue,
                at.key("run"),
                format!("`{script}`: a Windows machine runs PowerShell scripts (`.ps1`)"),
            ));
        }
        Step::Playbook(p) => {
            out.push(
                Problem::new(
                    Code::InvalidValue,
                    at.key("playbook"),
                    format!("`{p}`: a playbook runs on a Linux machine; a Windows machine's steps are PowerShell scripts"),
                )
                .fix("`{ run: <script>.ps1 }`"),
            );
        }
        Step::Reboot => {
            out.push(Problem::new(
                Code::InvalidValue,
                at.clone(),
                "a Windows machine restarts from its own script, not a `reboot` step",
            ));
        }
        _ => {}
    }
}

/// A Linux machine's step: scripts are shell, playbooks YAML, apt's fields on apt's families,
/// and account names as `useradd` takes them.
fn linux_step(at: &FieldPath, m: &Machine, step: &Step, out: &mut Problems) {
    match step {
        Step::Run { script, .. } if !has_extension(script.as_str(), "sh") => {
            out.push(
                Problem::new(Code::InvalidValue, at.key("run"), format!("`{script}`: a step runs a shell script (`.sh`)"))
                    .fix("a playbook is `{ playbook: <file>.yml }`"),
            );
        }
        Step::Playbook(p) if !has_extension(p.as_str(), "yml") && !has_extension(p.as_str(), "yaml") => {
            out.push(Problem::new(
                Code::InvalidValue,
                at.key("playbook"),
                format!("`{p}`: a playbook is a `.yml` file"),
            ));
        }
        Step::Repository(r) if !apt(m) => {
            let field = match (&r.suite, &r.components) {
                (Some(_), _) => Some("suite"),
                (None, Some(_)) => Some("components"),
                (None, None) => None,
            };
            if let Some(field) = field {
                let os = m.os.map(|o| o.to_string()).unwrap_or_default();
                out.push(
                    Problem::new(
                        Code::InvalidValue,
                        at.key("repository").key(field),
                        format!("`{field}` is apt's, and `{os}` doesn't install with apt"),
                    )
                    .fix(format!("leave `{field}` out")),
                );
            }
        }
        Step::User(u) => account(&at.key("user").key("name"), u.name.as_str(), out),
        Step::Group(g) => account(&at.key("group").key("name"), g.name.as_str(), out),
        _ => {}
    }
}

/// Whether the machine's OS installs with apt (Debian, Ubuntu, Kali).
fn apt(m: &Machine) -> bool {
    m.os.is_some_and(|o| matches!(o.os().family, "debian" | "ubuntu" | "kali"))
}

/// A Linux account's name: lowercase letters, digits and `_.-`, from a letter or `_`, as
/// `useradd` takes it.
fn account(at: &FieldPath, name: &str, out: &mut Problems) {
    let ok = name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_.-".contains(c));
    if !ok {
        out.push(
            Problem::new(
                Code::InvalidValue,
                at.clone(),
                format!("`{name}`: a Linux account's name is lowercase letters, digits and `_.-`, starting with a letter or `_`"),
            )
            .fix(format!("`{}`", name.to_ascii_lowercase().replace(' ', "-"))),
        );
    }
}
