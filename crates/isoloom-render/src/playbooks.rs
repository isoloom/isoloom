//! The controller's script that runs the environment's playbooks (`provision:`): one script for
//! every target that runs playbooks (Vagrant, Proxmox, the clouds), so they all retry the same
//! way.
//!
//! A playbook whose only failures are unreachable hosts (a Windows host drops WinRM for a
//! minute after a reboot, an SSH daemon restarts) runs again on those hosts only: Ansible writes
//! them to `<playbook>.retry` and the next attempt takes `--limit @<playbook>.retry`, after a
//! pause, up to [`ATTEMPTS`] attempts in all. A task that fails is never retried: the setup
//! stops on it. Every attempt and pause is logged, so the output explains what happened.

use isoloom_spec::model::Spec;
use isoloom_spec::shell;

/// Attempts at a playbook in all: the first run and up to two retries on unreachable hosts.
pub const ATTEMPTS: u32 = 3;

/// Where Ansible writes the `.retry` files on the controller.
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
    let mut script = format!(
        "set -e\nmkdir -p /tmp/isoloom-facts {RETRY_DIR}\n{ENVIRONMENT} ANSIBLE_RETRY_FILES_ENABLED=True ANSIBLE_RETRY_FILES_SAVE_PATH={RETRY_DIR}\n{HELPERS}"
    );
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
/// over WinRM). [`script`] adds the `.retry` files, in [`RETRY_DIR`].
const ENVIRONMENT: &str = "export PATH=/opt/ansible/bin:$PATH ANSIBLE_HOST_KEY_CHECKING=False \
ANSIBLE_GATHERING=smart ANSIBLE_FORKS=20 ANSIBLE_PIPELINING=True ANSIBLE_CACHE_PLUGIN=jsonfile \
ANSIBLE_CACHE_PLUGIN_CONNECTION=/tmp/isoloom-facts ANSIBLE_CACHE_PLUGIN_TIMEOUT=7200";

/// The script's shell helpers (POSIX sh, safe under `set -e`):
/// - `retry_download`: role downloads (Galaxy, from GitHub) fail now and then on the network;
///   tried 5 times, with a growing pause (`ISOLOOM_RETRY_PAUSE` seconds, 15 by default).
/// - `play_outcome <log>`: the PLAY RECAP's verdict: `failed` when a task failed on any host,
///   `unreachable` when the only failures are unreachable hosts, `none` otherwise (no recap: the
///   run stopped before it, a syntax error say).
/// - `run_play <playbook> <command...>`: the command with the playbook, then, while the outcome
///   is `unreachable` and Ansible named the hosts in `<playbook>.retry`, again on those hosts
///   (`--limit @<retry file>`) after a pause of 4 × `ISOLOOM_RETRY_PAUSE` (60 s by default), up
///   to [`ATTEMPTS`] attempts in all. Its exit code is the last attempt's.
pub const HELPERS: &str = r#"_isoloom_pause=${ISOLOOM_RETRY_PAUSE:-15}
_isoloom_attempts=3
retry_download() {
  _isoloom_n=1
  until "$@"; do
    [ "$_isoloom_n" -lt 5 ] || return 1
    echo "isoloom: download failed; trying again in $((_isoloom_n * _isoloom_pause)) s ($((_isoloom_n + 1))/5)"
    sleep $((_isoloom_n * _isoloom_pause))
    _isoloom_n=$((_isoloom_n + 1))
  done
}
play_outcome() {
  awk '
    { gsub(/\033\[[0-9;]*m/, "") }
    /^PLAY RECAP/ { recap = 1; next }
    recap && / : ok=/ {
      for (i = 1; i <= NF; i++) {
        split($i, kv, "=")
        if (kv[1] == "failed") failed += kv[2]
        if (kv[1] == "unreachable") unreachable += kv[2]
      }
    }
    END { if (failed > 0) print "failed"; else if (unreachable > 0) print "unreachable"; else print "none" }
  ' "$1"
}
run_play() {
  _isoloom_book=$1
  shift
  _isoloom_retry="${ANSIBLE_RETRY_FILES_SAVE_PATH:-.}/${_isoloom_book%.*}.retry"
  _isoloom_log=$(mktemp)
  _isoloom_rcf=$(mktemp)
  _isoloom_n=1
  rm -f "$_isoloom_retry"
  while :; do
    if [ "$_isoloom_n" -eq 1 ]; then
      { "$@" "$_isoloom_book" && echo 0 > "$_isoloom_rcf" || echo $? > "$_isoloom_rcf"; } 2>&1 | tee "$_isoloom_log"
    else
      { "$@" "$_isoloom_book" --limit "@$_isoloom_retry" && echo 0 > "$_isoloom_rcf" || echo $? > "$_isoloom_rcf"; } 2>&1 | tee "$_isoloom_log"
    fi
    _isoloom_rc=$(cat "$_isoloom_rcf")
    [ "$_isoloom_rc" -ne 0 ] || break
    _isoloom_outcome=$(play_outcome "$_isoloom_log")
    if [ "$_isoloom_outcome" != unreachable ] || [ ! -s "$_isoloom_retry" ]; then
      echo "isoloom: $_isoloom_book failed (exit $_isoloom_rc, recap: $_isoloom_outcome): not retried, only unreachable hosts are"
      break
    fi
    _isoloom_hosts=$(tr '\n' ' ' < "$_isoloom_retry" | sed 's/ *$//')
    if [ "$_isoloom_n" -ge "$_isoloom_attempts" ]; then
      echo "isoloom: $_isoloom_book: hosts still unreachable after $_isoloom_n attempts: $_isoloom_hosts"
      break
    fi
    _isoloom_n=$((_isoloom_n + 1))
    echo "isoloom: $_isoloom_book: unreachable: $_isoloom_hosts; waiting $((4 * _isoloom_pause)) s, then attempt $_isoloom_n/$_isoloom_attempts on them (--limit @$_isoloom_retry)"
    sleep $((4 * _isoloom_pause))
  done
  rm -f "$_isoloom_log" "$_isoloom_rcf"
  return "$_isoloom_rc"
}
"#;

#[cfg(all(test, unix))]
mod tests;
