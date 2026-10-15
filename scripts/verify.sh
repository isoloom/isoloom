#!/bin/sh
# Run the official validators on generated files, inside the pinned toolchain image
# (tools/verify/Dockerfile), and compare the failures with the baseline
# (tools/verify/baseline.json): a failure the baseline doesn't know fails the run.
#
#   scripts/verify.sh [--update-baseline] [TREE]
#
# TREE is a folder of generated files, such as scripts/generate-all.sh writes. When it holds
# `.isoloom/` folders, only the files under them are checked (the rest is the projects' own
# input, which the generated files refer to). Without TREE, every example is generated first,
# into $VERIFY_OUT/tree.
#
# Each file goes to its format's validator: `docker compose config`; `terraform init
# -backend=false` and `terraform validate`, tflint (AWS, Azure, Google rulesets), and
# `cloud-init schema` on the cloud-config documents a module builds; `vagrant validate
# --ignore-provider`; kubeconform on manifests, on charts rendered by `helm template` (after
# `helm lint`) and on kustomizations built by kustomize; `cloud-init schema`; hadolint;
# ansible-inventory and ansible-lint; shellcheck; PSScriptAnalyzer. The runner that does this
# is tools/verify/runner, handed to the image with the tree.
#
# Written to $VERIFY_OUT (default target/verify):
#   results.jsonl   a line per file and validator: file, validator, status (pass, warn, fail,
#                   error, skip), message, rule (what the baseline matches), and for a failure,
#                   `known` (in the baseline)
#   new.jsonl       the failures the baseline doesn't know
#   stale.jsonl     baseline entries no failure matches any more
#   summary.txt     counts per validator and the new failures, also printed
# --update-baseline rewrites tools/verify/baseline.json with today's failures instead, keeping
# the reasons already written (a new cause gets an empty reason, to fill in).
#
# The same on macOS, Linux and Windows (Git Bash) hosts: no host path reaches Docker. The tree
# goes into the container as a tar stream on stdin and the results come back on stdout; the
# only mount is a named volume, the cache of Terraform providers and Kubernetes schemas, so a
# run is offline once the first has filled it.
#
# Environment:
#   VERIFY_OUT             where results go (default target/verify)
#   VERIFY_CACHE           the cache volume (default isoloom-verify-cache)
#   VERIFY_JOBS            validators at once (default: the container's CPUs)
#   VERIFY_TERRAFORM_JOBS  Terraform modules at once (default: half of VERIFY_JOBS)
#   ISOLOOM                passed to generate-all.sh
#
# Exit status: 0 when every failure is in the baseline, 1 when some aren't, 2 when the run
# itself failed.
set -eu

update=false
if [ "${1:-}" = "--update-baseline" ]; then
  update=true
  shift
fi
root=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "${VERIFY_OUT:-$root/target/verify}"
out=$(cd "${VERIFY_OUT:-$root/target/verify}" && pwd)
cache=${VERIFY_CACHE:-isoloom-verify-cache}

if [ $# -gt 0 ]; then
  tree=$(cd "$1" && pwd)
  case $out/ in
    "$tree"/*) echo "verify: VERIFY_OUT ($out) can't be inside the tree" >&2; exit 2 ;;
  esac
else
  tree=$out/tree
  "$root/scripts/generate-all.sh" "$tree" || exit 2
fi

# Docker sees no host path, so none may be rewritten for it (Git Bash and MSYS2 rewrite
# arguments that look like paths: /cache, /work). Set after generate-all.sh, whose cargo and
# isoloom do take host paths.
MSYS_NO_PATHCONV=1
MSYS2_ARG_CONV_EXCL='*'
export MSYS_NO_PATHCONV MSYS2_ARG_CONV_EXCL

# The image: tagged with a checksum of what it is built from, so a changed Dockerfile builds
# a new one, and an unchanged one is built once per host.
sum=$(cat "$root/tools/verify/Dockerfile" "$root/tools/verify/requirements.txt" | cksum | awk '{ print $1 }')
image=isoloom-verify:$sum
if ! docker info > /dev/null 2>&1; then
  echo "verify: Docker isn't running" >&2
  exit 2
fi
if ! docker image inspect "$image" > /dev/null 2>&1; then
  echo "verify: building $image (tools/verify/Dockerfile)" >&2
  # The build context goes as a tar stream too, so no host path reaches Docker.
  tar -cf - -C "$root/tools/verify" Dockerfile requirements.txt |
    docker build --platform linux/amd64 -t "$image" - >&2 || exit 2
fi
id=$(docker image inspect -f '{{.Id}}' "$image")

# The tree and the runner, in one archive, at /work/tree and /work/verify. The tree is
# copied first, so its own name can't clash with the runner's.
work=$out/.work
rm -rf "$work" "$out/results.jsonl" "$out/new.jsonl" "$out/stale.jsonl" "$out/summary.txt" "$out/baseline.json"
mkdir -p "$work"
cp -R "$tree" "$work/tree"
COPYFILE_DISABLE=1 tar -cf "$work/in.tar" --exclude=.terraform --exclude=.vagrant --exclude=.git \
  --exclude=__pycache__ -C "$work" tree -C "$root/tools" verify
rm -rf "$work/tree"

flags=""
if $update; then flags=--update; fi
status=0
docker run --rm -i --platform linux/amd64 -v "$cache:/cache" \
  -e VERIFY_JOBS -e VERIFY_TERRAFORM_JOBS -e "VERIFY_IMAGE=$image $id" \
  "$image" sh -c '
    set -u
    mkdir -p /work /out
    tar -xf - -C /work --no-same-owner --warning=no-unknown-keyword || exit 2
    code=0
    python3 /work/verify/runner --tree /work/tree --out /out --baseline /work/verify/baseline.json $1 >&2 || code=$?
    tar -cf - -C /out .
    exit $code
  ' verify "$flags" < "$work/in.tar" > "$work/out.tar" || status=$?
tar -xf "$work/out.tar" -C "$out" 2> /dev/null || true

if [ ! -f "$out/summary.txt" ]; then
  echo "verify: the run failed (exit status $status), no results" >&2
  exit 2
fi
cat "$out/summary.txt"
if $update; then
  cp "$out/baseline.json" "$root/tools/verify/baseline.json"
  echo "verify: baseline rewritten (tools/verify/baseline.json)" >&2
fi
rm -rf "$work"
case $status in
  0) exit 0 ;;
  3) exit 1 ;;
  *) echo "verify: the runner failed (exit status $status)" >&2; exit 2 ;;
esac
