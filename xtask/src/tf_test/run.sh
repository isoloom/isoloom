# cargo xtask tf-test, inside the validator image (tools/verify/Dockerfile): `terraform test` in
# each module /work/modules lists, TF_TEST_JOBS at once, the providers from the cache volume
# (/cache, shared with `cargo xtask verify`). For each module, in the list's order, it prints a
# `@@module <folder>` line, a `@@status <terraform test's exit status, or init-failed>` line,
# Terraform's JSON lines, then what else was said as `@@log <line>`. Progress goes to stderr.
set -u
cache=/cache/terraform/plugins
mkdir -p "$cache" /out
printf 'plugin_cache_dir = "%s"\nplugin_cache_may_break_dependency_lock_file = true\n' "$cache" > /tmp/online.rc
{ cat /tmp/online.rc; printf 'provider_installation {\n  filesystem_mirror {\n    path = "%s"\n  }\n}\n' "$cache"; } > /tmp/offline.rc

# Two passes over the modules, TF_TEST_JOBS at once: every `init`, then every module's tests.
# `init` writes the module's lock file, and a module lists its project's files (`fileset`), its
# sibling modules' included: a test planned while another module's init is writing there can
# read a file that is gone the next instant. Tests write nothing in the tree.

# A module's `init`, from the cache alone (a provider the cache lacks is downloaded once, one
# module at a time). Its status is written only when it fails.
init_one() {
  n=$1 dir=$2
  export TF_DATA_DIR="/tmp/tf-data/$n" TF_CLI_CONFIG_FILE=/tmp/offline.rc
  if ! cd "/work/tree/$dir" 2>/dev/null; then
    echo "no such module" > "/out/$n.log"; echo missing > "/out/$n.status"; return 0
  fi
  if ! terraform init -backend=false -input=false -no-color > "/out/$n.log" 2>&1; then
    if ! flock /cache/terraform/.tf-test.lock env TF_CLI_CONFIG_FILE=/tmp/online.rc \
      terraform init -backend=false -input=false -no-color > "/out/$n.log" 2>&1; then
      echo init-failed > "/out/$n.status"; echo "tf-test: $dir: init failed" >&2
    fi
  fi
}

# A module's tests, which plan against mocked providers: no credentials, no cloud. A provider
# that starts too slowly on a loaded machine fails the run before any assertion: that is retried,
# twice at most.
test_one() {
  n=$1 dir=$2
  [ ! -f "/out/$n.status" ] || return 0
  export TF_DATA_DIR="/tmp/tf-data/$n" TF_CLI_CONFIG_FILE=/tmp/offline.rc
  cd "/work/tree/$dir" || return 0
  for try in 1 2 3; do
    : > "/out/$n.log"
    terraform test -json -no-color > "/out/$n.jsonl" 2> "/out/$n.log"
    code=$?
    grep -q 'timeout while waiting for plugin to start' "/out/$n.jsonl" "/out/$n.log" || break
    echo "tf-test: $dir: a provider started too slowly (try $try)" >&2
  done
  echo "$code" > "/out/$n.status"
  echo "tf-test: $dir: $([ "$code" = 0 ] && echo pass || echo fail)" >&2
}
export -f init_one test_one
for pass in init_one test_one; do
  awk '{ print NR, $0 }' /work/modules | xargs -P "${TF_TEST_JOBS:-4}" -L 1 bash -c "$pass"' "$0" "$1"'
done

n=0
while read -r dir; do
  n=$((n + 1))
  printf '@@module %s\n@@status %s\n' "$dir" "$(cat "/out/$n.status" 2>/dev/null || echo none)"
  [ ! -f "/out/$n.jsonl" ] || cat "/out/$n.jsonl"
  [ ! -s "/out/$n.log" ] || sed 's/^/@@log /' "/out/$n.log"
done < /work/modules
