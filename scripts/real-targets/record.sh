#!/usr/bin/env bash
# The record of one real-target run (docs/real-targets.md, "Records"), built from the JSON events
# `isoloom run|test|down --json` wrote into DIR (run.jsonl, test.jsonl, down.jsonl; a missing
# file is a phase that never ran). Writes DIR/record.json and prints a Markdown summary for the
# job summary. Exits 0 even when the run failed: the workflow's own steps carry the verdict.
#
#   RECORD_TARGET=vagrant RECORD_EXAMPLE=solo-web RECORD_PROVIDER=libvirt \
#     scripts/real-targets/record.sh "$RUNNER_TEMP/record"
#
# Environment: RECORD_TARGET and RECORD_EXAMPLE (required), RECORD_CLOUD (cloud targets),
# RECORD_PROVIDER (the Vagrant provider), RECORD_MODULE (a Terraform module folder, for the
# providers' versions from its lock file), ISOLOOM (the binary, default `isoloom`), and the
# runner's GITHUB_* variables for the commit and the run's URL.
set -euo pipefail

dir=${1:?usage: record.sh <dir with run.jsonl, test.jsonl, down.jsonl>}
: "${RECORD_TARGET:?RECORD_TARGET is required}" "${RECORD_EXAMPLE:?RECORD_EXAMPLE is required}"
isoloom=${ISOLOOM:-isoloom}

# One phase's events, as a JSON object: ok (the `result` event's), the error document, and the
# first and last event times. Lines that aren't JSON (a crash's stray output) are skipped.
phase() {
  local file="$dir/$1.jsonl"
  if [ ! -s "$file" ]; then
    jq -n --arg p "$1" '{phase: $p, ran: false, ok: null, error: null, started: null, finished: null, duration_seconds: null}'
    return
  fi
  jq -R 'fromjson? // empty' "$file" | jq -s --arg p "$1" '
    [.[] | .time? // empty] as $t
    | {phase: $p, ran: true,
       ok: ([.[] | select(.event? == "result") | .ok] | last // false),
       error: ([.[] | .error? // empty | {code, message}] | first),
       started: ($t | first), finished: ($t | last),
       duration_seconds: (if ($t | length) > 0 then ($t | last | fromdateiso8601) - ($t | first | fromdateiso8601) else null end)}'
}

# Every check `isoloom test` reported, in order.
checks() {
  local file="$dir/test.jsonl"
  if [ ! -s "$file" ]; then echo '[]'; return; fi
  jq -R 'fromjson? // empty' "$file" | jq -s '[.[] | select(.event? == "check") | {position, name, ok, detail}]'
}

# A tool's version line as a JSON string, or null when the tool isn't here.
version_of() {
  local line
  if command -v "$1" >/dev/null 2>&1 && line=$("$@" 2>/dev/null | head -n 1) && [ -n "$line" ]; then
    jq -n --arg v "$line" '$v'
  else
    echo null
  fi
}

# The providers locked in a Terraform module (`registry.terraform.io/hashicorp/aws` -> version).
terraform_providers() {
  local lock="${RECORD_MODULE:-}/.terraform.lock.hcl"
  if [ -z "${RECORD_MODULE:-}" ] || [ ! -f "$lock" ]; then echo '{}'; return; fi
  awk '/^provider "/ { gsub(/"/, "", $2); p = $2 } /^  version *=/ && p { gsub(/"/, "", $3); print p "\t" $3; p = "" }' "$lock" |
    jq -R 'split("\t") | {(.[0]): .[1]}' | jq -s 'add // {}'
}

# The Vagrant plugins installed (libvirt's provider is one), one per line, as a JSON array.
vagrant_plugins() {
  if ! command -v vagrant >/dev/null 2>&1; then echo null; return; fi
  vagrant plugin list 2>/dev/null | jq -R 'select(length > 0 and (startswith(" ") | not))' | jq -s . || echo null
}

# The Proxmox VE version, from its API, when the endpoint and a token are in the environment.
proxmox_version() {
  local body
  if [ -z "${PROXMOX_VE_ENDPOINT:-}" ] || [ -z "${PROXMOX_VE_API_TOKEN:-}" ]; then echo null; return; fi
  if body=$(curl -fsSk --max-time 10 -H "Authorization: PVEAPIToken=$PROXMOX_VE_API_TOKEN" \
    "${PROXMOX_VE_ENDPOINT%/}/api2/json/version" 2>/dev/null); then
    jq '.data.version // null' <<<"$body" 2>/dev/null || echo null
  else
    echo null
  fi
}

# The versions of what ran the environment: the tools the target uses and, for Terraform, the
# providers it locked. Tools that aren't on this runner are left out.
versions() {
  case "$RECORD_TARGET" in
    vagrant | hybrid | docker-vm)
      jq -n \
        --argjson vagrant "$(version_of vagrant --version)" \
        --argjson plugins "$(vagrant_plugins)" \
        --argjson virtualbox "$(version_of VBoxManage --version)" \
        --argjson libvirt "$(version_of virsh --version)" \
        --argjson qemu "$(version_of qemu-system-x86_64 --version)" \
        '{vagrant: $vagrant, vagrant_plugins: $plugins, virtualbox: $virtualbox, libvirt: $libvirt, qemu: $qemu}'
      ;;
    *)
      jq -n \
        --argjson terraform "$(version_of terraform version)" \
        --argjson providers "$(terraform_providers)" \
        --argjson proxmox "$(proxmox_version)" \
        '{terraform: $terraform, terraform_providers: $providers, proxmox: $proxmox}'
      ;;
  esac | jq 'with_entries(select(.value != null and .value != {} and .value != []))'
}

# The commit Isoloom was built from: the runner's, else the checkout's.
commit() {
  if [ -n "${GITHUB_SHA:-}" ]; then echo "$GITHUB_SHA"; return; fi
  git rev-parse HEAD 2>/dev/null || echo unknown
}

run_url=""
if [ -n "${GITHUB_RUN_ID:-}" ]; then
  run_url="${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-}/actions/runs/$GITHUB_RUN_ID"
fi

jq -n \
  --arg target "$RECORD_TARGET" --arg example "$RECORD_EXAMPLE" \
  --arg cloud "${RECORD_CLOUD:-}" --arg provider "${RECORD_PROVIDER:-}" \
  --arg commit "$(commit)" \
  --argjson isoloom_version "$(version_of "$isoloom" --version)" \
  --arg runner "${RUNNER_NAME:-$(hostname)}" --arg os "$(uname -srm)" \
  --arg run_url "$run_url" \
  --argjson versions "$(versions)" \
  --argjson phases "[$(phase run), $(phase test), $(phase down)]" \
  --argjson checks "$(checks)" '
  def nonempty: if . == "" then null else . end;
  ([$phases[] | .started // empty] | first) as $start
  | ([$phases[] | .finished // empty] | last) as $end
  | {format: "isoloom-real-target/1",
     target: $target, cloud: ($cloud | nonempty), provider: ($provider | nonempty),
     example: $example,
     isoloom: {commit: $commit, version: $isoloom_version},
     runner: {name: $runner, os: $os}, workflow_run: ($run_url | nonempty),
     versions: $versions,
     started: $start, finished: $end,
     duration_seconds: (if $start and $end then ($end | fromdateiso8601) - ($start | fromdateiso8601) else null end),
     result: (if ([$phases[] | .ok == true] | all) and ([$checks[] | .ok] | all) then "pass" else "fail" end),
     phases: $phases,
     checks: $checks,
     passed: ([$checks[] | select(.ok)] | length),
     failed: ([$checks[] | select(.ok | not)] | length)}' > "$dir/record.json"

# The job summary: the verdict, each phase, each check.
jq -r '
  def cell: tostring | gsub("\\|"; "\\|") | gsub("\n"; " ");
  "### \(.target)\(if .cloud then " (" + .cloud + ")" else "" end)\(if .provider then " on " + .provider else "" end): \(.example), \(.result)",
  "",
  "Commit `\(.isoloom.commit[0:12])`, \(.duration_seconds // "?") s, \(.passed) checks passed, \(.failed) failed.",
  "",
  "| phase | ok | seconds | error |", "|---|---|---:|---|",
  (.phases[] | "| \(.phase) | \(if .ran | not then "not run" elif .ok then "yes" else "**no**" end) | \(.duration_seconds // "") | \(.error.message // "" | cell) |"),
  "",
  (if (.checks | length) > 0 then
     ("| from | check | ok |", "|---|---|---|",
      (.checks[] | "| \(.position | cell) | \(.name | cell) | \(if .ok then "yes" else "**no**" end) |"))
   else empty end)' "$dir/record.json"
