#!/usr/bin/env bash
# Points the consumers' Isoloom dependencies at this Isoloom checkout, for local work across the
# repositories, without editing their committed Cargo.toml pins or Cargo.lock.
#
#   scripts/dev-link.sh link   [CONSUMER_DIR...]   build against this checkout's crates
#   scripts/dev-link.sh unlink [CONSUMER_DIR...]   back to the git pins
#   scripts/dev-link.sh status [CONSUMER_DIR...]
#
# The consumers default to the Isothrum and launcher checkouts next to the main Isoloom checkout;
# ISOLOOM_ISOTHRUM_DIR and ISOLOOM_LAUNCHER_DIR override them.
#
# Each workspace of a consumer whose committed Cargo.lock pins Isoloom by git gets a
# `.cargo/config.toml` with:
#   - a [patch."https://github.com/isoloom/isoloom"] entry per Isoloom crate the workspace depends
#     on directly, pointing at crates/<name> here (the others follow through the path dependencies
#     inside this workspace);
#   - a `resolver.lockfile-path` to a lockfile of its own, seeded from the committed one, so the
#     committed Cargo.lock keeps the git pin.
# Cargo reads `.cargo/config.toml` in the folder it runs from and its parents. So the workspace at
# the repository root, or the only one, gets its file at the repository root (the launcher runs
# `cargo --manifest-path src-tauri/Cargo.toml` from there); any other workspace gets one in its
# own folder. Their paths go to the repository's .git/info/exclude: local, never committed, and
# shared by every branch and worktree, so they stay there after an unlink (another worktree may
# still be linked). Running link or unlink again gives the same result.
set -euo pipefail

GIT_URL=https://github.com/isoloom/isoloom
MARKER='# Written by isoloom scripts/dev-link.sh.'
ALT_DIR=isoloom-dev-link
BLOCK_BEGIN='# >>> isoloom dev-link (scripts/dev-link.sh)'
BLOCK_END='# <<< isoloom dev-link'

isoloom_dir=$(cd "$(dirname "$0")/.." && pwd -P)
# The folder holding the main Isoloom checkout, so a worktree's copy finds the same consumers.
if common=$(git -C "$isoloom_dir" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
  code_dir=$(dirname "$(dirname "$common")")
else
  code_dir=$(dirname "$isoloom_dir")
fi

usage() {
  sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

# The workspaces of consumer $1 (folders relative to it, `.` for its root) whose committed
# Cargo.lock pins Isoloom by git.
workspaces() {
  git -C "$1" ls-files -- Cargo.lock '*/Cargo.lock' | while IFS= read -r lock; do
    if [ -f "$1/$lock" ] && grep -q "^source = \"git+${GIT_URL}[?#\"]" "$1/$lock"; then
      dirname "$lock"
    fi
  done
}

# The Isoloom crates lockfile $1's own packages (those without a source) depend on directly.
direct_isoloom_crates() {
  awk -v url="git+$GIT_URL" '
    function flush(   i) {
      if (name == "") return
      if (index(src, url) == 1) git[name] = 1
      else if (src == "") for (i = 1; i <= n; i++) direct[deps[i]] = 1
      name = ""
    }
    /^\[\[package\]\]$/ { flush(); src = ""; n = 0; indeps = 0; next }
    /^\[/ { flush(); indeps = 0; next }
    indeps && /^\]/ { indeps = 0; next }
    indeps { d = $0; gsub(/^[ \t]*"|",?[ \t]*$/, "", d); split(d, w, " "); deps[++n] = w[1]; next }
    /^name = / { name = $3; gsub(/"/, "", name); next }
    /^source = / { src = $3; gsub(/"/, "", src); next }
    /^dependencies = \[$/ { indeps = 1; next }
    END { flush(); for (c in git) if (c in direct) print c }
  ' "$1" | sort
}

has_local_crate() {
  grep -q "^name = \"$1\"$" "$isoloom_dir/crates/$1/Cargo.toml" 2>/dev/null
}

exclude_file() {
  git -C "$1" rev-parse --path-format=absolute --git-path info/exclude
}

# The paths (relative to consumer $1, with a leading /) listed in its dev-link exclude block.
linked_paths() {
  local f
  f=$(exclude_file "$1")
  [ -f "$f" ] || return 0
  awk -v b="$BLOCK_BEGIN" -v e="$BLOCK_END" '$0 == b { on = 1; next } $0 == e { on = 0 } on' "$f"
}

# Adds the remaining arguments to consumer $1's dev-link exclude block.
add_excludes() {
  local c=$1 f tmp lines
  shift
  f=$(exclude_file "$c")
  lines=$({ linked_paths "$c"; printf '%s\n' "$@"; } | sort -u)
  mkdir -p "$(dirname "$f")"
  touch "$f"
  tmp="$f.dev-link.$$"
  awk -v b="$BLOCK_BEGIN" -v e="$BLOCK_END" '$0 == b { skip = 1; next } $0 == e { skip = 0; next } !skip' "$f" >"$tmp"
  { echo "$BLOCK_BEGIN"; printf '%s\n' "$lines"; echo "$BLOCK_END"; } >>"$tmp"
  mv "$tmp" "$f"
}

# Fails when $1/$2 is a file dev-link must not touch: tracked, or written by someone else.
guard() {
  if git -C "$1" ls-files --error-unmatch -- "$2" >/dev/null 2>&1; then
    echo "  $2 is committed in $1: add the [patch] there by hand" >&2
    return 1
  fi
  if [ -f "$1/$2" ] && ! grep -qxF "$MARKER" "$1/$2"; then
    echo "  $1/$2 exists and dev-link did not write it: move it away first" >&2
    return 1
  fi
}

unlink_consumer() {
  local c=$1 path cfg
  for path in $(linked_paths "$c"); do
    case $path in
      */config.toml)
        cfg="$c$path"
        if [ -f "$cfg" ] && grep -qxF "$MARKER" "$cfg"; then rm -f "$cfg"; fi
        ;;
      */"$ALT_DIR"/) rm -rf "${c:?}$path" ;;
    esac
    rmdir "$(dirname "$c$path")" 2>/dev/null || true
  done
}

# Writes the config for workspace $3 of consumer $1 into folder $2 (both relative to $1), patching
# the crates given after them.
write_config() {
  local c=$1 at=$2 ws=$3 base crate
  shift 3
  base=$c
  [ "$at" = . ] || base="$c/$at"
  mkdir -p "$base/.cargo/$ALT_DIR"
  cp "$c/$ws/Cargo.lock" "$base/.cargo/$ALT_DIR/Cargo.lock"
  {
    echo "$MARKER"
    echo "# Local only (in .git/info/exclude); \`dev-link.sh unlink\` removes it."
    echo "# isoloom: $isoloom_dir"
    echo "# workspace: $ws"
    echo
    echo "# The Isoloom crates this workspace pins by git come from the local checkout instead."
    echo "[patch.\"$GIT_URL\"]"
    for crate in "$@"; do
      echo "$crate = { path = \"$isoloom_dir/crates/$crate\" }"
    done
    echo
    echo "# The linked resolution has its own lockfile, so the committed Cargo.lock keeps the git pin."
    echo "[resolver]"
    echo "lockfile-path = \"$base/.cargo/$ALT_DIR/Cargo.lock\""
  } >"$base/.cargo/config.toml"
}

link_consumer() {
  local c=$1 list count ws at rel crates crate missing excludes=""
  list=$(workspaces "$c")
  if [ -z "$list" ]; then
    echo "  no workspace pins Isoloom by git: nothing to link"
    unlink_consumer "$c"
    return 0
  fi
  count=$(printf '%s\n' "$list" | wc -l | tr -d ' ')
  for ws in $list; do
    at=$ws
    if [ "$ws" = . ] || [ "$count" = 1 ]; then at=.; fi
    rel=.cargo/config.toml
    [ "$at" = . ] || rel="$at/.cargo/config.toml"
    guard "$c" "$rel" || return 1
  done
  unlink_consumer "$c"
  for ws in $list; do
    at=$ws
    if [ "$ws" = . ] || [ "$count" = 1 ]; then at=.; fi
    crates=""
    missing=""
    for crate in $(direct_isoloom_crates "$c/$ws/Cargo.lock"); do
      if has_local_crate "$crate"; then crates="$crates $crate"; else missing="$missing $crate"; fi
    done
    if [ -n "$missing" ]; then
      echo "  $ws: stays on the git pin for$missing (not a crate of $isoloom_dir)"
    fi
    [ -n "$crates" ] || continue
    # shellcheck disable=SC2086 # the crate names are words
    write_config "$c" "$at" "$ws" $crates
    rel=/.cargo
    [ "$at" = . ] || rel="/$at/.cargo"
    excludes="$excludes $rel/config.toml $rel/$ALT_DIR/"
    echo "  $ws:$crates -> $isoloom_dir (${rel#/}/config.toml)"
  done
  # shellcheck disable=SC2086 # the paths have no spaces: they are relative to the repository
  [ -z "$excludes" ] || add_excludes "$c" $excludes
}

status_consumer() {
  local c=$1 path found=""
  for path in $(linked_paths "$c"); do
    case $path in
      */config.toml)
        if [ -f "$c$path" ]; then
          found=1
          echo "  ${path#/}: $(sed -n 's/^# isoloom: //p' "$c$path") ($(awk '/^\[patch/ { on = 1; next } /^$/ { on = 0 } on { printf "%s ", $1 }' "$c$path" | sed 's/ $//'))"
        fi
        ;;
    esac
  done
  [ -n "$found" ] || echo "  git pins (not linked)"
}

main() {
  local cmd=${1:-}
  [ $# -gt 0 ] && shift
  case $cmd in link | unlink | status) ;; *) usage ;; esac
  local consumers status=0 c
  if [ $# -gt 0 ]; then consumers=$(printf '%s\n' "$@"); else consumers=$(printf '%s\n' "${ISOLOOM_ISOTHRUM_DIR:-$code_dir/isothrum}" "${ISOLOOM_LAUNCHER_DIR:-$code_dir/cyber-ctf-launcher}"); fi
  while IFS= read -r c; do
    if [ ! -d "$c/.git" ] && [ ! -f "$c/.git" ]; then
      echo "$c: not a git checkout, skipped"
      continue
    fi
    c=$(cd "$c" && pwd -P)
    echo "$c:"
    case $cmd in
      link) link_consumer "$c" || status=1 ;;
      unlink) unlink_consumer "$c" && echo "  git pins (not linked)" ;;
      status) status_consumer "$c" ;;
    esac
  done <<<"$consumers"
  return $status
}

main "$@"
