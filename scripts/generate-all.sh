#!/bin/sh
# Generate every example, or every spec folder given, for every target it can be generated for,
# into one tree that scripts/verify.sh checks:
#
#   <out>/<example>/            a copy of the example's folder: its spec, and the files the
#                               generated ones refer to (build contexts, provision scripts),
#                               which the validators follow
#   <out>/<example>/.isoloom/   what `isoloom generate` writes, fresh: the committed outputs
#                               are left out of the copy
#   <out>/generate.log          what `isoloom generate` said, the targets it skipped included
#
#   scripts/generate-all.sh <out> [spec folder...]
#
# The tree is rebuilt from scratch, so it holds only this run's files. A spec that fails to
# generate stops the run.
#
# Environment: ISOLOOM (the binary; default: built with cargo, into CARGO_TARGET_DIR).
set -eu

out=${1:?usage: generate-all.sh <out> [spec folder...]}
shift
root=$(cd "$(dirname "$0")/.." && pwd)
if [ -z "${ISOLOOM:-}" ]; then
  cargo build -q -p isoloom --manifest-path "$root/Cargo.toml"
  ISOLOOM=${CARGO_TARGET_DIR:-$root/target}/debug/isoloom
  # Windows hosts (Git Bash, MSYS2): the binary has its extension.
  [ -x "$ISOLOOM" ] || ISOLOOM=$ISOLOOM.exe
fi
[ $# -gt 0 ] || set -- "$root"/examples/*/

rm -rf "$out"
mkdir -p "$out"
out=$(cd "$out" && pwd)
log=$out/generate.log
: > "$log"

for spec in "$@"; do
  n=$(basename "$spec")
  cp -R "$spec" "$out/$n"
  rm -rf "$out/$n/.isoloom" "$out/$n"/.isoloom-*
  echo "== $n" >> "$log"
  if ! "$ISOLOOM" generate "$out/$n" >> "$log" 2>&1; then
    echo "generate-all: $n failed to generate (see $log)" >&2
    exit 1
  fi
  [ -d "$out/$n/.isoloom" ] || echo "generate-all: $n: no target can be generated" >&2
done
echo "generate-all: $# specs into $out" >&2
