//! Isoloom's runner (design §3): the process of a container whose recipe runs services, generated
//! per machine in POSIX `sh` and installed as the image's entrypoint. The engine's init (Compose's
//! `init: true`, a pod sharing its process namespace) is PID 1 and reaps what the runner leaves.
//!
//! It runs the recipe's `start` steps, links each daemon's log files to the container's output,
//! then:
//! - **one daemon:** `exec`s it, dropping to its user, so the daemon is the container's process;
//! - **several (or one and the machine's command):** starts each in recipe order, each line of
//!   its output prefixed `<unit> | `, then waits. `SIGTERM` or `SIGINT` goes to every daemon.
//!   When one exits, it stops the others and exits with that one's status, and the engine
//!   restarts the container: a half-running machine never looks ready.
//!
//! The machine's command reaches it as its arguments (Compose's `command:`, a pod's `args:`).

use isoloom_plan::configure::{ContainerProcess, Daemon, Resource, ServiceSource, Stream};
use isoloom_spec::shell::quote;
use isoloom_spec::syntax::{AbsPath, UserName};

use super::{Configured, DIR, shell};
use crate::{GeneratedFile, header};

/// Where machine `machine`'s runner is generated, from the project folder.
#[must_use]
pub fn path(machine: &str) -> String {
    format!("{DIR}/{machine}/run")
}

/// Machine `c`'s runner as a generated file, when its container runs `process` through it.
#[must_use]
pub fn file(c: &Configured<'_>, process: &ContainerProcess) -> Option<GeneratedFile> {
    Some(GeneratedFile {
        path: path(c.machine()),
        contents: script(c, process)?,
    })
}

/// Drops to a user (empty: stays root), in a folder (empty: stays), then `exec`s the command:
/// `setpriv` where util-linux has it, else `su`.
const AS: &str = r#"isoloom_as() {
  _user=$1 _cwd=$2
  shift 2
  if [ -n "$_cwd" ]; then cd "$_cwd"; fi
  if [ -z "$_user" ] || [ "$_user" = root ]; then exec "$@"; fi
  if command -v setpriv >/dev/null 2>&1; then
    exec setpriv --reuid="$_user" --regid="$(id -g "$_user")" --init-groups -- "$@"
  fi
  _line='exec'
  for _arg do
    _line="$_line '$(printf '%s' "$_arg" | sed "s/'/'\\\\''/g")'"
  done
  exec su -s /bin/sh -c "$_line" "$_user"
}
"#;

/// Starts and watches several processes, each line of their output prefixed with its name.
const SUPERVISE: &str = r#"isoloom_dir=$(mktemp -d)
isoloom_pids=
# Prints each line of its input as `NAME | line`.
isoloom_prefix() {
  while IFS= read -r _line || [ -n "$_line" ]; do printf '%s | %s\n' "$1" "$_line"; done
}
# Starts NAME as USER in CWD: ARGS..., its output through a FIFO, so its own PID is kept.
isoloom_start() {
  mkfifo "$isoloom_dir/$1"
  isoloom_prefix "$1" <"$isoloom_dir/$1" &
  _name=$1
  shift
  isoloom_as "$@" >"$isoloom_dir/$_name" 2>&1 &
  isoloom_pids="$isoloom_pids $!"
}
# Whether process PID runs (not ended, nor a zombie waiting for this runner).
isoloom_alive() {
  [ -r "/proc/$1/stat" ] && [ "$(sed 's/.*) //' "/proc/$1/stat" | cut -c 1)" != Z ]
}
# Stops every process and waits for them.
isoloom_stop() {
  for _pid in $isoloom_pids; do kill -TERM "$_pid" 2>/dev/null || true; done
  for _pid in $isoloom_pids; do wait "$_pid" 2>/dev/null || true; done
}
trap 'isoloom_stop; exit 143' TERM INT
# Waits until one ends, then stops the others and exits with its status.
isoloom_watch() {
  while :; do
    for _pid in $isoloom_pids; do
      isoloom_alive "$_pid" && continue
      _status=0
      wait "$_pid" || _status=$?
      echo "isoloom: process $_pid ended (status $_status): stopping the others" >&2
      isoloom_stop
      exit "$_status"
    done
    sleep 1
  done
}
"#;

/// Machine `c`'s runner, when its container runs `process` through it (one daemon, or several).
#[must_use]
pub fn script(c: &Configured<'_>, process: &ContainerProcess) -> Option<String> {
    let (daemons, command) = match process {
        ContainerProcess::One(d) => (std::slice::from_ref(d), false),
        ContainerProcess::Supervised { daemons, command } => (daemons.as_slice(), command.is_some()),
        _ => return None,
    };
    let start = start(c);
    let logs: String = daemons.iter().map(logs).collect();
    let run = if let ([one], false) = (daemons, command) {
        format!("{}\n", invocation(c, "isoloom_as", one))
    } else {
        let mut each = String::new();
        for d in daemons {
            each.push_str(&invocation(c, &format!("isoloom_start {}", quote(d.unit.as_str())), d));
            each.push('\n');
        }
        let command = if command { "isoloom_start command '' '' \"$@\"\n" } else { "" };
        format!("{SUPERVISE}{each}{command}isoloom_watch\n")
    };
    Some(format!(
        "#!/bin/sh\n{}# The process of machine `{}`'s container: its services, run by Isoloom's runner.\nset -eu\n{start}{AS}{logs}{run}",
        header("#"),
        c.machine()
    ))
}

/// The recipe's `start` steps, run at each start before the services, through the shell
/// library (an entry's; an app has none in 1.0).
fn start(c: &Configured<'_>) -> String {
    let calls: String = c
        .recipe()
        .start
        .iter()
        .filter_map(|r| shell::call(c, r))
        .map(|line| {
            if shell::literal_dollar(&line) {
                format!("# shellcheck disable=SC2016\n{line}")
            } else {
                line
            }
        })
        .collect();
    if calls.is_empty() {
        return String::new();
    }
    format!(". /usr/local/lib/isoloom/configure.sh\ncd /opt/isoloom\n{calls}")
}

/// Links each of the daemon's log files to the container's output (a fresh link at each start).
fn logs(d: &Daemon) -> String {
    d.logs
        .iter()
        .map(|l| {
            let path = quote(l.path.as_str());
            let stream = if l.stream == Stream::Stderr { "/dev/stderr" } else { "/dev/stdout" };
            format!("mkdir -p \"$(dirname {path})\" && rm -f {path} && ln -s {stream} {path}\n")
        })
        .collect::<Vec<_>>()
        .concat()
}

/// `function`, then the daemon's user, folder (a service Isoloom defines may name one) and
/// foreground form, each quoted.
fn invocation(c: &Configured<'_>, function: &str, d: &Daemon) -> String {
    let user = quote(d.user.as_ref().map(UserName::as_str).unwrap_or_default());
    let cwd = c.recipe().resources().find_map(|r| match r {
        Resource::Service(s) if s.unit == d.unit => match &s.source {
            ServiceSource::Defined { cwd, .. } => cwd.as_ref(),
            _ => None,
        },
        _ => None,
    });
    let args: Vec<String> = d.foreground.args().iter().map(|a| quote(a)).collect();
    format!("{function} {user} {} {}", quote(cwd.map(AbsPath::as_str).unwrap_or_default()), args.join(" "))
}

#[cfg(test)]
mod tests;
