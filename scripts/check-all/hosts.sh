# The hosts of scripts/check-all.sh: where each part runs, how the sources get there, and the
# summary. Sourced by check-all.sh.
# shellcheck shell=bash
# The remote commands are built here on purpose (SC2029).
# shellcheck disable=SC2029

rssh() { ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=30 "$@"; }
rscp() { scp -q -o BatchMode=yes -o ConnectTimeout=10 "$@"; }
LINUX_TARGET="${ISOLOOM_LINUX_USER:-florianamette}@${ISOLOOM_LINUX_HOST:-10.0.0.10}"
WIN_TARGET="${ISOLOOM_WIN_USER:-florianamette}@${ISOLOOM_WIN_HOST:-10.0.0.40}"
WIN_DIR=${ISOLOOM_WIN_DIR:-C:/isoloom-check-all}
DOCKER_VOLUME=isoloom-check-all-linux-docker

# Reads a host's stream (see leg.sh): each gate's output goes to its own log, its result to the
# host's results file.
parse_leg() {
  local host=$1 log="$logs/$1.log" line repo gate status secs note
  while IFS= read -r line; do
    line=${line%$'\r'}
    case $line in
      @@begin$'\t'*)
        IFS=$'\t' read -r _ repo gate <<<"$line"
        log="$logs/$host-$repo-$gate.log"
        : >"$log"
        ;;
      @@result$'\t'*)
        IFS=$'\t' read -r _ repo gate status secs note <<<"$line"
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$host" "$repo" "$gate" "$status" "$secs" "$note" >>"$logs/$host.results"
        printf '  %-12s %-9s %-15s %-5s %s\n' "$host" "$repo" "$gate" "$status" "$(duration "$secs")"
        log="$logs/$host.log"
        ;;
      *) printf '%s\n' "$line" >>"$log" ;;
    esac
  done
}

duration() {
  printf '%d:%02d' $(($1 / 60)) $(($1 % 60))
}

# A host that could not run at all: one row for it.
host_failed() {
  printf '%s\t-\t-\t%s\t0\t%s\n' "$1" "$2" "$3" >>"$logs/$1.results"
  printf '  %-12s %s: %s\n' "$1" "$2" "$3"
}

# The folder of repository $1 here, and its name on the other hosts.
repo_dir() {
  case $1 in isoloom) echo "$isoloom_dir" ;; isothrum) echo "$isothrum_dir" ;; launcher) echo "$launcher_dir" ;; esac
}
remote_name() {
  local pair
  for pair in $REMOTE_NAMES; do [ "${pair%%:*}" = "$1" ] && echo "${pair#*:}"; done
}

# The repositories a remote host needs: Isoloom always (the runner scripts, the linked crates).
shipped_repos() {
  echo isoloom
  echo "$repos" | tr , '\n' | grep -vx isoloom
}

# Writes to $4 a tar of repository folder $2 (named $1) as git sees it, plus the dev-link files of
# a linked consumer with their paths moved to the copy's: Isoloom to $3/isoloom, the consumer to
# $3/<its name>.
pack() {
  local name=$1 dir=$2 root=$3 out=$4 stage path linked
  (cd "$dir" && git ls-files -z -co --exclude-standard | while IFS= read -r -d '' path; do
    if [ -e "$path" ] || [ -L "$path" ]; then printf '%s\0' "$path"; fi
  done) >"$out.list"
  COPYFILE_DISABLE=1 tar --no-xattrs -cf "$out" -C "$dir" --null -T "$out.list" || return 1
  stage="$out.stage"
  rm -rf "$stage" && mkdir -p "$stage"
  for path in $(dev_link_configs "$dir"); do
    linked=$(sed -n 's/^# isoloom: //p' "$dir/$path")
    [ "$linked" = "$isoloom_dir" ] || echo "note: $dir is linked to $linked; the copy uses $isoloom_dir" >&2
    mkdir -p "$stage/$(dirname "$path")/isoloom-dev-link"
    sed -e "s|$linked/|$root/isoloom/|g" -e "s|$dir/|$root/$name/|g" "$dir/$path" >"$stage/$path"
    cp "$dir/$(dirname "$path")/isoloom-dev-link/Cargo.lock" "$stage/$(dirname "$path")/isoloom-dev-link/"
  done
  if [ -n "$(ls -A "$stage")" ]; then
    COPYFILE_DISABLE=1 tar --no-xattrs -rf "$out" -C "$stage" . || return 1
  fi
  rm -rf "$stage" "$out.list"
}

# The dev-link configs present in checkout $1 (paths relative to it).
dev_link_configs() {
  local f
  f=$(git -C "$1" rev-parse --path-format=absolute --git-path info/exclude 2>/dev/null) || return 0
  [ -f "$f" ] || return 0
  awk '/^# >>> isoloom dev-link/ { on = 1; next } /^# <<< isoloom dev-link/ { on = 0 } on && /config.toml$/' "$f" |
    while IFS= read -r path; do
      path=${path#/}
      if [ -f "$1/$path" ] && grep -q '^# Written by isoloom scripts/dev-link.sh' "$1/$path"; then echo "$path"; fi
    done
}

host_mac() {
  if [ "$(uname -s)" != Darwin ]; then
    host_failed mac skip "not a Mac"
    return
  fi
  run_leg mac full | parse_leg mac
}

# Copies repository $1 to $3/src/$2 on the Linux host, replacing the previous copy.
copy_linux() {
  local tar="$logs/linux-$1.tar"
  pack "$2" "$(repo_dir "$1")" "$3/src" "$tar" 2>>"$logs/linux.log" || return 1
  rssh "$LINUX_TARGET" "rm -rf '$3/src/$2' && mkdir -p '$3/src/$2' && tar -xf - --warning=no-unknown-keyword -C '$3/src/$2'" \
    <"$tar" >>"$logs/linux.log" 2>&1 || return 1
  rm -f "$tar"
}

host_linux() {
  local root r name
  if ! root=$(rssh "$LINUX_TARGET" 'cd && pwd' 2>>"$logs/linux.log"); then
    host_failed linux unreachable "not reachable (set ISOLOOM_LINUX_USER and authorize the key)"
    return
  fi
  root="${ISOLOOM_LINUX_DIR:-$root/isoloom-check-all}"
  for r in $(shipped_repos); do
    name=$(remote_name "$r")
    if ! copy_linux "$r" "$name" "$root"; then
      host_failed linux FAIL "copying $r failed (see linux.log)"
      return
    fi
  done
  rssh "$LINUX_TARGET" "CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4} bash '$root/src/isoloom/scripts/check-all.sh' --leg linux --profile platform --repos '$repos' --isoloom '$root/src/isoloom' --isothrum '$root/src/isothrum' --launcher '$root/src/cyber-ctf-launcher' --target-root '$root/target'" </dev/null 2>&1 | parse_leg linux
}

# Runs PowerShell script $1 on the Windows host (encoded, so neither cmd nor PowerShell quoting
# applies).
win_ps() {
  local encoded
  encoded=$(printf '%s' "\$ProgressPreference = 'SilentlyContinue'; $1" | iconv -f UTF-8 -t UTF-16LE | base64 | tr -d '\n')
  rssh "$WIN_TARGET" "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand $encoded"
}

# Copies repository $1 to <WIN_DIR>/src/$2 on the Windows host, replacing the previous copy.
copy_windows() {
  local tar="$logs/windows-$1.tar" dest="$WIN_DIR/src/$2"
  pack "$2" "$(repo_dir "$1")" "$WIN_DIR/src" "$tar" 2>>"$logs/windows.log" || return 1
  rscp "$tar" "$WIN_TARGET:$WIN_DIR/incoming/$2.tar" >>"$logs/windows.log" 2>&1 || return 1
  win_ps "if (Test-Path '$dest') { Remove-Item -Recurse -Force '$dest' }; New-Item -ItemType Directory -Force '$dest' | Out-Null; tar -xf '$WIN_DIR/incoming/$2.tar' -C '$dest'; if (\$LASTEXITCODE) { exit 1 }; Remove-Item '$WIN_DIR/incoming/$2.tar'" \
    </dev/null >>"$logs/windows.log" 2>&1 || return 1
  rm -f "$tar"
}

host_windows() {
  local r name q list
  if ! win_ps "New-Item -ItemType Directory -Force '$WIN_DIR/incoming' | Out-Null" </dev/null >>"$logs/windows.log" 2>&1; then
    host_failed windows unreachable "not reachable (set ISOLOOM_WIN_USER and authorize the key)"
    return
  fi
  for r in $(shipped_repos); do
    name=$(remote_name "$r")
    if ! copy_windows "$r" "$name"; then
      host_failed windows FAIL "copying $r failed (see windows.log)"
      return
    fi
  done
  q="'"
  list=${repos//,/$q,$q}
  win_ps "& '$WIN_DIR/src/isoloom/scripts/check-all/leg-windows.ps1' -Root '$WIN_DIR' -Repos '$list'" </dev/null 2>&1 | parse_leg windows
}

host_linux_docker() {
  local image tag mounts="" d linked
  if ! docker info >/dev/null 2>&1; then
    host_failed linux-docker unreachable "Docker is not running"
    return
  fi
  tag=$(shasum <"$here/check-all/linux.Dockerfile" | cut -c1-12)
  image="isoloom-check-all-linux:$tag"
  if ! docker image inspect "$image" >/dev/null 2>&1; then
    echo "  linux-docker: building $image (once)"
    docker build -t "$image" -f "$here/check-all/linux.Dockerfile" "$here/check-all" >>"$logs/linux-docker.log" 2>&1 ||
      { host_failed linux-docker FAIL "image build failed (see linux-docker.log)"; return; }
  fi
  # The repositories at their own paths, so dev-link's absolute paths hold; the launcher's
  # node_modules shadowed by a volume (the host's has macOS binaries).
  for d in "$isoloom_dir" "$isothrum_dir" "$launcher_dir"; do
    [ -d "$d" ] && mounts="$mounts -v $d:$d"
    for linked in $(dev_link_configs "$d"); do
      linked=$(sed -n 's/^# isoloom: //p' "$d/$linked")
      case " $mounts " in *" -v $linked:$linked "*) ;; *) mounts="$mounts -v $linked:$linked" ;; esac
    done
  done
  # shellcheck disable=SC2086 # the mounts are words; the paths have no spaces
  docker run --rm $mounts \
    -v "$DOCKER_VOLUME:/check-all" -v "$DOCKER_VOLUME-node-modules:$launcher_dir/node_modules" \
    -e CARGO_HOME=/check-all/cargo-home -e COREPACK_HOME=/check-all/corepack \
    -e npm_config_store_dir=/check-all/pnpm-store -e COREPACK_ENABLE_DOWNLOAD_PROMPT=0 \
    -e CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" \
    "$image" bash "$isoloom_dir/scripts/check-all.sh" --leg linux-docker --profile platform --repos "$repos" \
    --isoloom "$isoloom_dir" --isothrum "$isothrum_dir" --launcher "$launcher_dir" --target-root /check-all/target \
    </dev/null 2>&1 | parse_leg linux-docker
}

clean_hosts() {
  local h
  for h in $(echo "$hosts" | tr , ' '); do
    case $h in
      linux) rssh "$LINUX_TARGET" "rm -rf ${ISOLOOM_LINUX_DIR:-\$HOME/isoloom-check-all}" && echo "linux: removed ${ISOLOOM_LINUX_DIR:-~/isoloom-check-all}" ;;
      windows) win_ps "if (Test-Path '$WIN_DIR') { Remove-Item -Recurse -Force '$WIN_DIR' }" </dev/null && echo "windows: removed $WIN_DIR" ;;
      linux-docker)
        docker volume rm "$DOCKER_VOLUME" "$DOCKER_VOLUME-node-modules" >/dev/null
        docker image ls -q isoloom-check-all-linux | sort -u | xargs docker image rm -f >/dev/null
        echo "linux-docker: removed the volumes and the image"
        ;;
      mac) echo "mac: the repositories' own build folders stay (cargo clean)" ;;
    esac
  done
}

summary() {
  local h total=0 failed
  echo
  printf '%-12s %-9s %-15s %-12s %6s  %s\n' HOST REPO GATE RESULT TIME NOTE
  for h in $(echo "$hosts" | tr , ' '); do
    [ -f "$logs/$h.results" ] || continue
    while IFS=$'\t' read -r _ repo gate status secs note; do
      printf '%-12s %-9s %-15s %-12s %6s  %s\n' "$h" "$repo" "$gate" "$status" "$(duration "$secs")" "$note"
    done <"$logs/$h.results"
    [ -f "$logs/$h.time" ] && printf '%-12s %-9s %-15s %-12s %6s\n' "$h" total "" "" "$(duration "$(cat "$logs/$h.time")")"
  done
  echo "logs: $logs"
  failed=$(cat "$logs"/*.results 2>/dev/null | awk -F '\t' '$4 == "FAIL" { print $1 "-" $2 "-" $3 }')
  for h in $failed; do
    [ -f "$logs/$h.log" ] || continue
    printf '\n--- %s (last 20 lines of %s)\n' "$h" "$logs/$h.log"
    tail -n 20 "$logs/$h.log"
  done
  [ -z "$failed" ] || total=1
  return $total
}

# Runs host $1's part, timing it.
timed_host() {
  local start
  start=$(date +%s)
  case $1 in
    mac) host_mac ;;
    linux) host_linux ;;
    windows) host_windows ;;
    linux-docker) host_linux_docker ;;
    *) host_failed "$1" FAIL "unknown host (mac, linux, windows, linux-docker)" ;;
  esac
  echo $(($(date +%s) - start)) >"$logs/$1.time"
  # A host that stopped before its first result (a lost connection, a container that didn't start).
  [ -f "$logs/$1.results" ] || host_failed "$1" FAIL "no result (see $1.log)"
}

main() {
  local h pids=""
  if [ -n "$clean" ]; then
    clean_hosts
    return
  fi
  # Physical paths, as dev-link.sh writes them.
  isothrum_dir=$(cd "$isothrum_dir" 2>/dev/null && pwd -P || echo "$isothrum_dir")
  launcher_dir=$(cd "$launcher_dir" 2>/dev/null && pwd -P || echo "$launcher_dir")
  logs=${logs:-${TMPDIR:-/tmp}/isoloom-check-all/$(date +%Y%m%d-%H%M%S)}
  logs=${logs%/}
  mkdir -p "$logs"
  echo "check-all: $repos on $hosts"
  echo "  isoloom  $isoloom_dir"
  echo "  isothrum $isothrum_dir"
  echo "  launcher $launcher_dir"
  # The remote hosts in the background; this machine's hosts one after the other.
  for h in $(echo "$hosts" | tr , ' '); do
    case $h in linux | windows)
      timed_host "$h" &
      pids="$pids $!"
      ;;
    esac
  done
  for h in $(echo "$hosts" | tr , ' '); do
    case $h in linux | windows) ;; *) timed_host "$h" ;; esac
  done
  for h in $pids; do wait "$h"; done
  summary
}
