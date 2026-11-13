//! A machine's VM form from its Dockerfile: the same spec, a true VM on a VM target. A machine
//! written as `app.build` (and no `app.steps` of its own) is translated into the explicit VM form
//! an author would write, `os` and `app.steps`, which every VM renderer then runs like any other:
//!
//! - `FROM`: the service matrix's entry ([`matrix`]: a distro is its OS; an official php,
//!   mariadb, mysql, postgres, python or node image is the install of that version and its
//!   image's conventions), else a fetched and vendored Dockerfile ([`sources`]) translated first.
//! - `RUN`, `COPY`, `ADD`, `WORKDIR`, `VOLUME`: shell steps, each run once, with `ENV` and `ARG`
//!   exported, as the `USER` in effect ([`translate`]). Multi-stage builds run the stages the last
//!   one needs on the same VM.
//! - `ENTRYPOINT` and `CMD`: the VM's one service, `isoloom-app`, run by a start script that sets
//!   the image's environment and folder. The daemons the steps' packages enabled are disabled,
//!   as the container ran its one process tree only.
//! - `LABEL`, `EXPOSE`, `HEALTHCHECK`, `STOPSIGNAL`: nothing (the spec's ports and healthcheck say
//!   it).
//!
//! What has no VM form (`FROM scratch`, `COPY --from=<another image>`, `RUN --mount`, `ONBUILD`,
//! a base no source gives) is refused, naming the file, the line and the instruction.

pub mod instruction;
pub mod matrix;
pub mod sources;
mod translate;

use std::fmt;

use isoloom_spec::syntax::{OsRef, Step};
use serde_json::{Value, json};

use self::instruction::Command;
pub use self::sources::{Fetched, Index, Origin, Sources, Vendored};
use self::translate::{Image, Run, argv, quote};

/// The marker folder a translated step's guard names.
const MARKERS: &str = "/var/lib/isoloom/dockerfile";

/// The script the VM's service runs: the image's environment and folder, then its command.
const START: &str = "/usr/local/lib/isoloom/app-start";

/// The VM's service.
const SERVICE: &str = "isoloom-app";

/// A Dockerfile's VM form: what an author would write as the machine's `os` and `app.steps`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Translation {
    /// The VM's OS.
    pub os: OsRef,
    /// The steps that make the VM what the image is.
    pub steps: Vec<Step>,
    /// `os` and `app` as YAML, the way a spec writes them: what the translation is, to read
    /// or to paste as the machine's explicit VM form.
    pub yaml: String,
}

/// Why a Dockerfile has no VM form: the file, the line, the instruction and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Untranslatable {
    /// The Dockerfile: its project path, or a fetched base's image and origin.
    pub file: String,
    /// The line (1-based; 0: the file as a whole).
    pub line: usize,
    /// The instruction's keyword (`COPY`), empty for the file as a whole.
    pub instruction: String,
    /// Why it has no VM form.
    pub reason: String,
    /// What gives the machine one.
    pub fix: String,
}

impl Untranslatable {
    pub(crate) fn at(file: &str, line: usize, instruction: &str, reason: String, fix: &str) -> Untranslatable {
        Untranslatable {
            file: file.to_owned(),
            line,
            instruction: instruction.to_owned(),
            reason,
            fix: fix.to_owned(),
        }
    }
}

impl fmt::Display for Untranslatable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.instruction.as_str()) {
            (0, _) => write!(f, "{}: {}", self.file, self.reason),
            (n, "") => write!(f, "{} line {n}: {}", self.file, self.reason),
            (n, i) => write!(f, "{} line {n} (`{i}`): {}", self.file, self.reason),
        }
    }
}

/// The build of a machine to translate.
#[derive(Debug, Clone, Copy)]
pub struct Build<'a> {
    /// The Dockerfile's project path (as problems name it).
    pub path: &'a str,
    /// Its text.
    pub text: &'a str,
    /// The build context, a project folder.
    pub context: &'a str,
    /// The build's arguments (`app.build.args`).
    pub args: &'a [(String, String)],
    /// The machine's own `os`, when the spec names one: the matrix installs on it when it can.
    pub os: Option<&'a str>,
}

/// `build`'s VM form, its bases from the matrix or `sources`.
///
/// # Errors
///
/// [`Untranslatable`]: the first instruction (of the file or a base it starts from) with no VM
/// form.
pub fn translate(build: &Build<'_>, sources: &Sources) -> Result<Translation, Untranslatable> {
    let mut run = Run::new(sources, build.os);
    let df = translate::Dockerfile {
        file: build.path,
        text: build.text,
        context: build.context,
        given: build.args,
    };
    let image = run.file(df, 0)?;
    let os: OsRef = image
        .os
        .parse()
        .map_err(|e| Untranslatable::at(build.path, 0, "", format!("`{}` is no OS Isoloom has: {e}", image.os), "report this to Isoloom"))?;
    let mut shells = vec![UNITS_BEFORE.to_owned()];
    shells.extend(run.steps);
    shells.push(UNITS_AFTER.to_owned());
    let service = start(&image);
    if let Some((script, _)) = &service {
        shells.push(script.clone());
    }
    let mut written: Vec<Value> = shells
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let marker = format!("{MARKERS}/{:03}", i + 1);
            json!({ "command": format!("{}\nmkdir -p {MARKERS} && touch {marker}\n", text.trim_end()), "creates": marker })
        })
        .collect();
    if let Some((_, user)) = service {
        let s = match user {
            Some(user) => json!({ "name": SERVICE, "run": [START], "cwd": image.workdir, "user": user }),
            None => json!({ "name": SERVICE, "run": [START], "cwd": image.workdir }),
        };
        written.push(json!({ "service": s }));
    }
    let steps: Vec<Step> = serde_json::from_value(Value::Array(written.clone()))
        .map_err(|e| Untranslatable::at(build.path, 0, "", format!("the VM form isn't a valid one: {e}"), "report this to Isoloom"))?;
    let form = json!({ "os": image.os, "app": { "steps": written } });
    let yaml = serde_yaml_ng::to_string(&form).map_err(|e| Untranslatable::at(build.path, 0, "", format!("the VM form doesn't write: {e}"), "report this to Isoloom"))?;
    Ok(Translation { os, steps, yaml })
}

/// The start script's step and the service's user, for an image that runs something (a bare
/// shell, as a distro image's command is, runs nothing: the VM is idle).
fn start(image: &Image) -> Option<(String, Option<String>)> {
    let shell = |c: &Command| match c {
        Command::Shell(text) => vec!["/bin/sh".to_owned(), "-c".to_owned(), text.clone()],
        Command::Exec(a) => a.clone(),
    };
    let argv_ = match (&image.entrypoint, &image.cmd) {
        (Some(e @ Command::Shell(_)), _) => shell(e),
        (Some(Command::Exec(e)), cmd) => e.iter().cloned().chain(cmd.iter().flat_map(shell)).collect(),
        (None, Some(c)) => shell(c),
        (None, None) => return None,
    };
    let idle = matches!(argv_.as_slice(), [one] if ["sh", "bash", "/bin/sh", "/bin/bash"].contains(&one.as_str()));
    if idle || argv_.is_empty() {
        return None;
    }
    let user = image.user.as_deref().map(|u| u.split(':').next().unwrap_or(u).to_owned());
    // A numeric user has no name the service could run as: the script switches to it.
    let (switch, user) = match user {
        Some(u) if u.chars().all(|c| c.is_ascii_digit()) => (format!("setpriv --reuid={u} --regid={u} --init-groups "), None),
        u => (String::new(), u),
    };
    let exports = image.env.iter().map(|(k, v)| format!("export {k}={}\n", quote(v))).collect::<Vec<_>>().concat();
    let script = format!("#!/bin/sh\n{exports}cd {}\nexec {switch}{}\n", quote(&image.workdir), argv(&argv_));
    let write = format!(
        "set -e\nmkdir -p {dir}\nprintf %s {b64} | base64 -d > {START}\nchmod 755 {START}",
        dir = START.rsplit_once('/').map_or("/", |(d, _)| d),
        b64 = isoloom_spec::shell::base64(script.as_bytes()),
    );
    Some((write, user))
}

/// Before the steps: the services enabled already (the OS's own).
const UNITS_BEFORE: &str = "set -e
mkdir -p /var/lib/isoloom/dockerfile
if command -v systemctl >/dev/null; then
  systemctl list-unit-files --state=enabled --type=service --no-legend | awk '{print $1}' | sort > /var/lib/isoloom/dockerfile/units-before
elif command -v rc-update >/dev/null; then
  rc-update show default | awk '{print $1}' | sort > /var/lib/isoloom/dockerfile/units-before
fi";

/// After the steps: the services they enabled disabled and stopped (a container ran its own
/// process tree, nothing a package starts), the OS's own kept.
const UNITS_AFTER: &str = "set -e
keep='^(ssh|sshd|systemd-|getty|serial-getty|cloud-|qemu-guest-agent|vbox|open-vm-tools|vmtoolsd|networking|rsyslog|cron|chrony|isoloom)'
if command -v systemctl >/dev/null; then
  systemctl list-unit-files --state=enabled --type=service --no-legend | awk '{print $1}' | sort > /var/lib/isoloom/dockerfile/units-after
  for u in $(comm -13 /var/lib/isoloom/dockerfile/units-before /var/lib/isoloom/dockerfile/units-after | grep -Ev \"$keep\" || true); do
    systemctl disable --now \"$u\" || true
  done
elif command -v rc-update >/dev/null; then
  rc-update show default | awk '{print $1}' | sort > /var/lib/isoloom/dockerfile/units-after
  for u in $(comm -13 /var/lib/isoloom/dockerfile/units-before /var/lib/isoloom/dockerfile/units-after | grep -Ev \"$keep\" || true); do
    rc-service \"$u\" stop || true
    rc-update del \"$u\" default || true
  done
fi";

#[cfg(test)]
mod tests;
