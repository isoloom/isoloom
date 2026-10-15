# One host's part of scripts/check-all.sh: the gates of the selected repositories, run here.
# Sourced by check-all.sh. Each gate prints `@@begin<TAB>repo<TAB>gate`, its output, then
# `@@result<TAB>repo<TAB>gate<TAB>status<TAB>seconds<TAB>note`; check-all.sh reads that stream
# (scripts/check-all/leg-windows.ps1 prints the same on Windows).
# shellcheck shell=bash

# Runs one gate: the command after the repository, gate name and folder. Returns its status.
gate() {
  local repo=$1 name=$2 dir=$3 start rc note=""
  shift 3
  printf '@@begin\t%s\t%s\n' "$repo" "$name"
  printf '$ %s\n' "$*"
  start=$(date +%s)
  (cd "$dir" && "$@") </dev/null 2>&1
  rc=$?
  [ $rc = 0 ] || note="exit $rc"
  printf '@@result\t%s\t%s\t%s\t%s\t%s\n' "$repo" "$name" "$([ $rc = 0 ] && echo pass || echo FAIL)" "$(($(date +%s) - start))" "$note"
  return $rc
}

# Records a gate that did not run (status $3: skip or FAIL), with the reason.
not_run() {
  printf '@@begin\t%s\t%s\n' "$1" "$2"
  printf '@@result\t%s\t%s\t%s\t0\t%s\n' "$1" "$2" "$3" "$4"
}

# The build folder of repository $1: one per repository, never shared.
use_target() {
  if [ -n "$target_root" ]; then
    export CARGO_TARGET_DIR="$target_root/$1"
  elif [ -n "${base_target:-}" ]; then
    export CARGO_TARGET_DIR="$base_target/$1"
  else
    unset CARGO_TARGET_DIR
  fi
}

# Isoloom: every gate on the full profile, the tests on the others.
leg_isoloom() {
  local d=$isoloom_dir
  use_target isoloom
  if [ "$1" = full ]; then
    gate isoloom fmt "$d" cargo fmt --all --check
    gate isoloom typos "$d" typos
    gate isoloom machete "$d" cargo machete
    gate isoloom clippy "$d" cargo clippy --workspace --all-targets -- -D warnings
    gate isoloom clippy-linux "$d" cargo clippy --workspace --all-targets --target x86_64-unknown-linux-gnu -- -D warnings
    gate isoloom clippy-windows "$d" cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings
    gate isoloom doc "$d" env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps
  fi
  gate isoloom test "$d" cargo test --workspace --no-fail-fast
}

# Isothrum's libraries (its root workspace; the desktop app is its own and moves out).
leg_isothrum() {
  local d=$isothrum_dir
  use_target isothrum
  if [ "$1" = full ]; then
    gate isothrum fmt "$d" cargo fmt --all --check
    gate isothrum clippy "$d" cargo clippy --workspace --all-targets -- -D warnings
  fi
  gate isothrum test "$d" cargo test --workspace --no-fail-fast
}

# The launcher's own gate, `pnpm check` (format, types, lint, the web tests, then the Rust
# format, lint and tests), after `pnpm install`. Every host runs it.
leg_launcher() {
  local d=$launcher_dir host=$1 version
  use_target launcher
  version=$(node --version 2>/dev/null || echo none)
  case $version in
    v24.*) ;;
    *) not_run launcher check FAIL "Node 24 needed, found $version (ISOLOOM_NODE_BIN)"; return ;;
  esac
  case $host in
    linux*)
      if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
        not_run launcher check skip "no Tauri system packages; as root: apt install $TAURI_PACKAGES"
        return
      fi
      ;;
  esac
  # The app embeds ../out at compile time; the tests don't need the real one (as in its CI).
  if [ ! -e "$d/out/index.html" ]; then
    mkdir -p "$d/out" && echo '<!doctype html>' >"$d/out/index.html"
  fi
  if gate launcher install "$d" pnpm install --frozen-lockfile; then
    gate launcher check "$d" pnpm check
  else
    not_run launcher check skip "install failed"
  fi
}

# Runs the selected repositories' gates here, as host $1 with profile $2 (full or platform).
run_leg() {
  local host=$1 profile=$2 repo dir
  # Where scripts/linux-host-setup.sh puts the tools (a non-interactive SSH shell lacks it).
  if [ "$host" = linux ] && [ -d "$HOME/.local/bin" ]; then PATH="$HOME/.local/bin:$PATH"; fi
  if [ -n "${ISOLOOM_NODE_BIN:-}" ]; then PATH="$ISOLOOM_NODE_BIN:$PATH"; fi
  if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
    # shellcheck source=/dev/null
    . "$HOME/.cargo/env"
  fi
  base_target=${CARGO_TARGET_DIR:-}
  echo "$host: $(uname -srm); $(cargo --version 2>&1); node $(node --version 2>&1)"
  for repo in $(echo "$repos" | tr , ' '); do
    case $repo in
      isoloom) dir=$isoloom_dir ;;
      isothrum) dir=$isothrum_dir ;;
      launcher) dir=$launcher_dir ;;
      *) not_run "$repo" - FAIL "unknown repository"; continue ;;
    esac
    if [ ! -d "$dir" ]; then
      not_run "$repo" - skip "not found at $dir"
      continue
    fi
    "leg_$repo" "$([ "$repo" = launcher ] && echo "$host" || echo "$profile")"
  done
}
