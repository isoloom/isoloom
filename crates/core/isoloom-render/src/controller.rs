//! The controller: the VM that runs the environment's playbooks (`provision:`) on every target
//! that runs playbooks (Vagrant, Proxmox, the clouds, `isoloom provision`), with one script, so
//! they all retry the same way.
//!
//! After a failed run, when no task failed on its own, the hosts that failed only because of the
//! connection run the playbook again, on those hosts only (`--limit @<playbook>.retry`), after a
//! pause, up to four runs in all:
//! - unreachable hosts (a Windows host dropping WinRM for a minute after a domain join or an
//!   update reboot);
//! - hosts whose task was cut by a dropped connection: a failure whose message is in
//!   the known list of dropped connections (WinRM's shell crashing mid-task, an SSH session
//!   reset).
//!
//! A task that failed on its own is real: the setup stops at once, nothing is retried. So does a
//! run with no recap (a playbook that doesn't parse). Every wait and run is logged, so the output
//! explains what happened.

use isoloom_spec::model::{Provision, Spec};
use isoloom_spec::shell;

/// Runs of a playbook in all: the first and up to three again.
const ATTEMPTS: u32 = 4;

/// Where Ansible writes its retry files (`<playbook>.retry`), and where `run_play` writes the
/// hosts a playbook runs again on, in the same file and format.
const RETRY_DIR: &str = "/tmp/isoloom-retry";

/// The controller's script running the environment's playbooks, in order, with the inventory
/// at `/etc/isoloom/inventory.ini` plus each step's own inventories, as it runs when the
/// environment comes up; `isoloom provision` runs it again on a running environment.
///
/// `limit`: only these machines (`isoloom provision web`); every machine when empty.
#[must_use]
pub fn provision_script(spec: &Spec, limit: &[String]) -> String {
    let limit = if limit.is_empty() {
        String::new()
    } else {
        format!(" --limit {}", shell::quote(&limit.join(",")))
    };
    let head = format!(
        "set -e\nmkdir -p /tmp/isoloom-facts\n{ENVIRONMENT}\nmkdir -p {RETRY_DIR}\nexport ANSIBLE_RETRY_FILES_ENABLED=True ANSIBLE_RETRY_FILES_SAVE_PATH={RETRY_DIR}\n{}",
        helpers()
    );
    let plays: String = spec.provision.iter().map(|step| play(step, &limit)).collect();
    head + &plays
}

/// One `provision:` step's lines: into its playbook's folder, its roles installed, the playbook
/// run through `run_play`.
fn play(step: &Provision, limit: &str) -> String {
    let (dir, file) = step.ansible.rsplit_once('/').unwrap_or((".", &step.ansible));
    let extra: String = step.inventory.iter().map(|i| format!(" -i /opt/isoloom/{i}")).collect::<Vec<_>>().concat();
    let requirements = match &step.requirements {
        Some(r) => format!("retry_download ansible-galaxy install -r /opt/isoloom/{r}\n"),
        None => "[ ! -f requirements.yml ] || retry_download ansible-galaxy install -r requirements.yml\n".into(),
    };
    format!(
        "cd /opt/isoloom/{dir}\n{requirements}run_play ansible-playbook -i /etc/isoloom/inventory.ini{extra}{}{limit} {file}\n",
        vars(step)
    )
}

/// A step's variables as one JSON `-e` (`-e k=v` splits values on spaces).
fn vars(step: &Provision) -> String {
    if step.vars.is_empty() {
        return String::new();
    }
    let json = serde_json::Value::Object(step.vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    format!(" -e {}", shell::quote(&json.to_string()))
}

/// Ansible's settings. Performance: facts gathered once and cached (the WinRM `setup` module is
/// slow and the environment's playbooks re-run across many imported plays), a fan-out across
/// hosts (the default 5 forks are too few for a multi-DC range), SSH steps pipelined (a no-op
/// over WinRM). `ANSIBLE_HOST_KEY_CHECKING=False` is a deliberate trade-off (reviewed, security
/// finding S5): the lab's hosts are freshly created and ephemeral, with no stable `known_hosts`
/// for the controller to trust, so host-key checking inside the lab network is off.
const ENVIRONMENT: &str = "export PATH=\"/opt/ansible/bin:$PATH\" ANSIBLE_HOST_KEY_CHECKING=False \
ANSIBLE_GATHERING=smart ANSIBLE_FORKS=20 ANSIBLE_PIPELINING=True ANSIBLE_CACHE_PLUGIN=jsonfile \
ANSIBLE_CACHE_PLUGIN_CONNECTION=/tmp/isoloom-facts ANSIBLE_CACHE_PLUGIN_TIMEOUT=7200";

/// Failure messages that are the connection dropping, not the task failing, as extended
/// regular expressions matched against a lowercased `fatal:` / `failed:` line. Ansible reports
/// most of these as a failed task rather than an unreachable host: WinRM's shell crashing
/// mid-task, a Windows host rebooting during a play (a domain join), an SSH session reset.
/// Lowercase; none may hold a `'` (the list is single-quoted in the script).
const TRANSIENT: &[&str] = &[
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
/// - `play_hosts <log>`: the hosts of a run's PLAY RECAP that failed, one line each, sorted:
///   `unreachable <host>`; `cut <host>` when every task that failed there was cut by the
///   connection (its `fatal:` line matches [`TRANSIENT`]); `real <host>` otherwise. A failure
///   Ansible ignored (`...ignoring`) doesn't count.
/// - `run_play <command...> <playbook>`: the command (the playbook its last argument), then,
///   while no host is `real` and some are `unreachable` or `cut`, again on those only
///   (`--limit @<playbook>.retry`, in `ANSIBLE_RETRY_FILES_SAVE_PATH`, [`RETRY_DIR`] by default)
///   after a pause of 4 × `ISOLOOM_RETRY_PAUSE` (60 s by default; 2 × when no host was
///   unreachable, only cut), up to [`ATTEMPTS`] runs in all. Its exit code is the last run's.
fn helpers() -> String {
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
      if (pending != "") { if (pending_real) real[pending] = 1; else cut[pending] = 1 }
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
      for (h in failed) print ((h in real || !(h in cut)) ? "real " : "cut ") h
      for (h in unreachable) if (!(h in failed)) print "unreachable " h
    }
  ' "$1" | sort
}
_isoloom_kind() {
  echo "$1" | awk -v k="$2" '$1 == k { printf "%s%s", s, $2; s = " " }'
}
run_play() {
  # The playbook is the last argument; Ansible names its retry file after it.
  for _isoloom_book; do :; done
  _isoloom_name=$(basename "$_isoloom_book")
  _isoloom_retry=${ANSIBLE_RETRY_FILES_SAVE_PATH:-/tmp/isoloom-retry}/${_isoloom_name%.*}.retry
  _isoloom_limit=
  _isoloom_log=$(mktemp)
  _isoloom_rcf=$(mktemp)
  _isoloom_n=1
  rm -f "$_isoloom_retry"
  while :; do
    { "$@" && echo 0 > "$_isoloom_rcf" || echo $? > "$_isoloom_rcf"; } 2>&1 | tee "$_isoloom_log"
    _isoloom_rc=$(cat "$_isoloom_rcf")
    [ "$_isoloom_rc" -ne 0 ] || break
    _isoloom_hosts=$(play_hosts "$_isoloom_log")
    _isoloom_real=$(_isoloom_kind "$_isoloom_hosts" real)
    _isoloom_gone=$(_isoloom_kind "$_isoloom_hosts" unreachable)
    _isoloom_cut=$(_isoloom_kind "$_isoloom_hosts" cut)
    _isoloom_again="$_isoloom_gone${_isoloom_gone:+${_isoloom_cut:+ }}$_isoloom_cut"
    if [ -n "$_isoloom_real" ]; then
      echo "isoloom: $_isoloom_name: a task failed on $_isoloom_real: not retried"
      break
    fi
    if [ -z "$_isoloom_again" ]; then
      echo "isoloom: $_isoloom_name failed (exit $_isoloom_rc) with no unreachable host or dropped connection: not retried"
      break
    fi
    if [ "$_isoloom_n" -ge "$_isoloom_attempts" ]; then
      echo "isoloom: $_isoloom_name: still unreachable after $_isoloom_n runs: $_isoloom_again; giving up"
      break
    fi
    echo "$_isoloom_again" | tr ' ' '\n' > "$_isoloom_retry"
    if [ -z "$_isoloom_limit" ]; then set -- "$@" --limit "@$_isoloom_retry"; _isoloom_limit=1; fi
    _isoloom_n=$((_isoloom_n + 1))
    if [ -n "$_isoloom_gone" ]; then
      _isoloom_wait=$((4 * _isoloom_pause))
      echo "isoloom: $_isoloom_name: unreachable: $_isoloom_again; waiting $_isoloom_wait s, then running it again on them ($_isoloom_n/$_isoloom_attempts)"
    else
      _isoloom_wait=$((2 * _isoloom_pause))
      echo "isoloom: $_isoloom_name: the play was cut by a dropped connection on $_isoloom_cut; waiting $_isoloom_wait s, then running it again on them ($_isoloom_n/$_isoloom_attempts)"
    fi
    sleep "$_isoloom_wait"
  done
  rm -f "$_isoloom_log" "$_isoloom_rcf"
  [ "$_isoloom_rc" -ne 0 ] || rm -f "$_isoloom_retry"
  return "$_isoloom_rc"
}
"#;

#[cfg(test)]
mod tests;
