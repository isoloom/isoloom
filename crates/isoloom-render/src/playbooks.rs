//! The controller's script that runs the environment's playbooks (`provision:`): one script for
//! every target that runs playbooks (Vagrant, Proxmox, the clouds), so they all retry the same
//! way.
//!
//! After a failed run, the hosts whose every failure was the connection (unreachable, or a task
//! cut by a dropped connection: a Windows host rebooting during a play drops WinRM, an SSH
//! session resets; see [`TRANSIENT`]) run the playbook again, on those hosts only
//! (`--limit @<playbook>.retry`), after a pause, up to [`ATTEMPTS`] attempts in all. A host where
//! a task failed on its own is never retried, and the setup stops on it once the retries are
//! done. Every attempt and pause is logged, so the output explains what happened.

use isoloom_spec::model::Spec;
use isoloom_spec::shell;

/// Attempts at a playbook in all: the first run and up to two retries.
pub const ATTEMPTS: u32 = 3;

/// Where the controller writes the hosts a playbook runs again on (`<playbook>.retry`, the
/// format of Ansible's own retry files, which can't be used: they mix in the hosts whose task
/// failed for real).
pub const RETRY_DIR: &str = "/tmp/isoloom-retry";

/// The script running the environment's playbooks, in order, with the inventory at
/// /etc/isoloom/inventory.ini plus each step's own inventories.
///
/// `limit`: only these machines (`isoloom provision web`); every machine when empty.
pub fn script(spec: &Spec, limit: &[String]) -> String {
    let limit = if limit.is_empty() {
        String::new()
    } else {
        format!(" --limit {}", shell::quote(&limit.join(",")))
    };
    let mut script = format!("set -e\nmkdir -p /tmp/isoloom-facts\n{ENVIRONMENT}\n{}", helpers());
    for step in &spec.provision {
        let (dir, file) = step.ansible.rsplit_once('/').unwrap_or((".", &step.ansible));
        let extra: String = step.inventory.iter().map(|i| format!(" -i /opt/isoloom/{i}")).collect();
        let requirements = match &step.requirements {
            Some(r) => format!("retry_download ansible-galaxy install -r /opt/isoloom/{r}\n"),
            None => "[ ! -f requirements.yml ] || retry_download ansible-galaxy install -r requirements.yml\n".into(),
        };
        script.push_str(&format!(
            "cd /opt/isoloom/{dir}\n{requirements}run_play {} ansible-playbook -i /etc/isoloom/inventory.ini{extra}{}{limit}\n",
            shell::quote(file),
            vars(step)
        ));
    }
    script
}

/// A step's variables as one JSON `-e` (`-e k=v` splits values on spaces).
fn vars(step: &isoloom_spec::model::Provision) -> String {
    if step.vars.is_empty() {
        return String::new();
    }
    match serde_json::to_string(&step.vars) {
        Ok(json) => format!(" -e {}", shell::quote(&json)),
        // JSON values always serialize; nothing is better than a broken argument.
        Err(_) => String::new(),
    }
}

/// Ansible's settings. Performance: facts gathered once and cached (the WinRM `setup` module is
/// slow and the environment's playbooks re-run across many imported plays), a fan-out across
/// hosts (the default 5 forks are too few for a multi-DC range), SSH steps pipelined (a no-op
/// over WinRM).
const ENVIRONMENT: &str = "export PATH=/opt/ansible/bin:$PATH ANSIBLE_HOST_KEY_CHECKING=False \
ANSIBLE_GATHERING=smart ANSIBLE_FORKS=20 ANSIBLE_PIPELINING=True ANSIBLE_CACHE_PLUGIN=jsonfile \
ANSIBLE_CACHE_PLUGIN_CONNECTION=/tmp/isoloom-facts ANSIBLE_CACHE_PLUGIN_TIMEOUT=7200";

/// Failure messages that are the connection dropping, not the task failing, as extended
/// regular expressions matched against a lowercased `fatal:` / `failed:` line. Ansible reports
/// most of these as a failed task rather than an unreachable host: WinRM's shell crashing
/// mid-task, a Windows host rebooting during a play (a domain join), an SSH session reset.
/// Matched without regard to case; none may hold a `'` (the list is single-quoted in the script).
pub const TRANSIENT: &[&str] = &[
    // WinRM: the shell or its HTTP transport went away under a task.
    "send_input failed",
    "the pipe has been ended",
    "winrmtransporterror",
    "winrmoperationtimeouterror",
    "read timeout",
    "bad http response returned from server",
    "(kerberos|ntlm|credssp).*500",
    "remote end closed connection",
    // TCP and SSH.
    "connection reset by peer",
    "connection refused",
    "connection aborted",
    "broken pipe",
    "timed out",
    "failed to connect to the host via ssh",
    "shared connection to .* closed",
];

/// The script's shell helpers (POSIX sh, safe under `set -e`):
/// - `retry_download`: role downloads (Galaxy, from GitHub) fail now and then on the network;
///   tried 5 times, with a growing pause (`ISOLOOM_RETRY_PAUSE` seconds, 15 by default).
/// - `play_hosts <log>`: the hosts of a run's PLAY RECAP that failed, one line each:
///   `retry <host>` when every failure it had was the connection (unreachable, or a failed task
///   whose message is [`TRANSIENT`]), `real <host>` when a task failed on its own (a failure
///   Ansible ignored, `...ignoring`, doesn't count).
/// - `run_play <playbook> <command...>`: the command with the playbook, then, while some hosts
///   are `retry`, again on those hosts only (`--limit @<dir>/<playbook>.retry`, a file written
///   here, in `ISOLOOM_RETRY_DIR`, [`RETRY_DIR`] by default) after a pause of
///   4 × `ISOLOOM_RETRY_PAUSE` (60 s by default), up to [`ATTEMPTS`] attempts in all. A `real`
///   host is never retried, and the play then fails whatever its retries do. Each attempt and
///   pause is logged.
pub fn helpers() -> String {
    format!(
        "_isoloom_pause=${{ISOLOOM_RETRY_PAUSE:-15}}\n_isoloom_attempts={ATTEMPTS}\n_isoloom_transient='{}'\n{HELPER_FUNCTIONS}",
        TRANSIENT.join("|")
    )
}

/// The functions of [`helpers`].
const HELPER_FUNCTIONS: &str = r#"retry_download() {
  _isoloom_n=1
  until "$@"; do
    [ "$_isoloom_n" -lt 5 ] || return 1
    echo "isoloom: download failed; trying again in $((_isoloom_n * _isoloom_pause)) s ($((_isoloom_n + 1))/5)"
    sleep $((_isoloom_n * _isoloom_pause))
    _isoloom_n=$((_isoloom_n + 1))
  done
}
play_hosts() {
  awk -v transient="$_isoloom_transient" '
    function commit() {
      if (pending != "") { if (pending_real) real[pending] = 1; else seen[pending] = 1 }
      pending = ""
    }
    { gsub(/\033\[[0-9;]*m/, "") }
    /^\.\.\.ignoring/ { pending = ""; next }
    { commit() }
    /^(fatal|failed): \[/ && !/UNREACHABLE!/ {
      pending = $0
      sub(/^(fatal|failed): \[/, "", pending)
      sub(/\].*/, "", pending)
      sub(/ -> .*/, "", pending)
      pending_real = (tolower($0) !~ transient)
      next
    }
    /^PLAY RECAP/ { recap = 1; next }
    recap && / : ok=/ {
      for (i = 2; i <= NF; i++) {
        split($i, kv, "=")
        if (kv[1] == "failed" && kv[2] > 0) failed[$1] = 1
        if (kv[1] == "unreachable" && kv[2] > 0) unreachable[$1] = 1
      }
    }
    END {
      commit()
      for (h in failed) print ((h in real) ? "real " : "retry ") h
      for (h in unreachable) if (!(h in failed)) print "retry " h
    }
  ' "$1" | sort
}
run_play() {
  _isoloom_book=$1
  shift
  _isoloom_limit="${ISOLOOM_RETRY_DIR:-/tmp/isoloom-retry}/${_isoloom_book%.*}.retry"
  _isoloom_log=$(mktemp)
  _isoloom_rcf=$(mktemp)
  _isoloom_n=1
  _isoloom_real_rc=0
  while :; do
    if [ "$_isoloom_n" -eq 1 ]; then
      { "$@" "$_isoloom_book" && echo 0 > "$_isoloom_rcf" || echo $? > "$_isoloom_rcf"; } 2>&1 | tee "$_isoloom_log"
    else
      { "$@" "$_isoloom_book" --limit "@$_isoloom_limit" && echo 0 > "$_isoloom_rcf" || echo $? > "$_isoloom_rcf"; } 2>&1 | tee "$_isoloom_log"
    fi
    _isoloom_rc=$(cat "$_isoloom_rcf")
    [ "$_isoloom_rc" -ne 0 ] || break
    _isoloom_hosts=$(play_hosts "$_isoloom_log")
    _isoloom_real=$(echo "$_isoloom_hosts" | awk '$1 == "real" { printf "%s%s", s, $2; s = " " }')
    _isoloom_retry=$(echo "$_isoloom_hosts" | awk '$1 == "retry" { printf "%s%s", s, $2; s = " " }')
    if [ -n "$_isoloom_real" ]; then
      echo "isoloom: $_isoloom_book: a task failed on $_isoloom_real: not retried"
      _isoloom_real_rc=$_isoloom_rc
    fi
    if [ -z "$_isoloom_retry" ]; then
      [ -n "$_isoloom_real" ] || echo "isoloom: $_isoloom_book failed (exit $_isoloom_rc) with no unreachable host or dropped connection: not retried"
      break
    fi
    if [ "$_isoloom_n" -ge "$_isoloom_attempts" ]; then
      echo "isoloom: $_isoloom_book: still unreachable or disconnected after $_isoloom_n attempts: $_isoloom_retry"
      break
    fi
    mkdir -p "$(dirname "$_isoloom_limit")"
    printf '%s\n' $_isoloom_retry > "$_isoloom_limit"
    _isoloom_n=$((_isoloom_n + 1))
    echo "isoloom: $_isoloom_book: unreachable or disconnected: $_isoloom_retry; waiting $((4 * _isoloom_pause)) s, then attempt $_isoloom_n/$_isoloom_attempts on them (--limit @$_isoloom_limit)"
    sleep $((4 * _isoloom_pause))
  done
  rm -f "$_isoloom_log" "$_isoloom_rcf"
  [ "$_isoloom_real_rc" -eq 0 ] || [ "$_isoloom_rc" -ne 0 ] || _isoloom_rc=$_isoloom_real_rc
  return "$_isoloom_rc"
}
"#;

#[cfg(all(test, unix))]
mod tests;
