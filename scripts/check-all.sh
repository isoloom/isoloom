#!/usr/bin/env bash
# Runs the gates of every repository (Isoloom, Isothrum, the Cyber CTF launcher) on one or more
# hosts, then prints one summary table. Exits 1 when a gate fails.
#
#   scripts/check-all.sh [--repos isoloom,isothrum,launcher] [--hosts mac,linux,windows]
#                        [--logs DIR] [--clean]
#
# Hosts (default: mac):
#   mac           this Mac, natively, every gate.
#   linux         a Linux host over SSH: ISOLOOM_LINUX_HOST (10.0.0.10), ISOLOOM_LINUX_USER
#                 (florianamette), sources and build folders under ISOLOOM_LINUX_DIR
#                 (~/isoloom-check-all).
#   windows       a Windows host over SSH, MSVC toolchain: ISOLOOM_WIN_HOST (10.0.0.40),
#                 ISOLOOM_WIN_USER (florianamette), ISOLOOM_WIN_DIR (C:/isoloom-check-all).
#                 scripts/windows-host-setup.ps1 prepares it.
#   linux-docker  a pinned Rust container on this machine (scripts/check-all/linux.Dockerfile),
#                 with the repositories mounted at their own paths.
# The other hosts run the platform gates (the tests, and the launcher's `pnpm check`) on a copy
# of the working trees as git sees them: tracked and new files, without the ignored ones. A
# consumer linked by scripts/dev-link.sh stays linked in the copy. The remote hosts run in the
# background while this machine runs its own; CARGO_BUILD_JOBS defaults to 4 there.
#
# Repositories: this Isoloom tree, and the Isothrum and launcher checkouts next to the main
# Isoloom checkout (ISOLOOM_ISOTHRUM_DIR, ISOLOOM_LAUNCHER_DIR). The launcher needs Node 24:
# ISOLOOM_NODE_BIN names its bin folder when the one on PATH is another version. A set
# CARGO_TARGET_DIR gets one subfolder per repository, so they never share one.
#
# --clean removes the hosts' build folders and copies (the remote folders, the Docker volumes and
# image).
# shellcheck source-path=SCRIPTDIR
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd -P)
isoloom_dir=$(dirname "$here")
if common=$(git -C "$isoloom_dir" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
  code_dir=$(dirname "$(dirname "$common")")
else
  code_dir=$(dirname "$isoloom_dir")
fi
isothrum_dir=${ISOLOOM_ISOTHRUM_DIR:-$code_dir/isothrum}
launcher_dir=${ISOLOOM_LAUNCHER_DIR:-$code_dir/cyber-ctf-launcher}

repos=isoloom,isothrum,launcher
hosts=mac
logs=""
clean=""
leg=""
profile=platform
target_root=""
# The names of the copies on the other hosts.
REMOTE_NAMES="isoloom:isoloom isothrum:isothrum launcher:cyber-ctf-launcher"
TAURI_PACKAGES="libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf"

usage() {
  sed -n '2,6p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

while [ $# -gt 0 ]; do
  case $1 in
    --repos) repos=${2:?}; shift ;;
    --hosts) hosts=${2:?}; shift ;;
    --logs) logs=${2:?}; shift ;;
    --clean) clean=1 ;;
    # The rest is how the hosts run their part: `--leg HOST` runs the gates here and prints them
    # as `@@begin` and `@@result` lines, which the machine that started the run reads.
    --leg) leg=${2:?}; shift ;;
    --profile) profile=${2:?}; shift ;;
    --isoloom) isoloom_dir=${2:?}; shift ;;
    --isothrum) isothrum_dir=${2:?}; shift ;;
    --launcher) launcher_dir=${2:?}; shift ;;
    --target-root) target_root=${2:?}; shift ;;
    -h | --help) usage ;;
    *) echo "unknown argument: $1" >&2; usage ;;
  esac
  shift
done

# shellcheck source=check-all/leg.sh
. "$here/check-all/leg.sh"
# shellcheck source=check-all/hosts.sh
. "$here/check-all/hosts.sh"

# Each call and its exit on one line: bash has read it whole, so editing this file during a run
# can't change what runs next.
if [ -n "$leg" ]; then run_leg "$leg" "$profile"; exit 0; fi
main; exit $?
