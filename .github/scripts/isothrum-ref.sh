#!/usr/bin/env bash
# Checks that ISOTHRUM_REF pins the Isothrum runner the real runs build: a release tag (vX.Y.Z)
# or a commit SHA, never a branch, so a run is reproducible (docs/dev/ci-real-hosts.md).
#
# Usage: isothrum-ref.sh required|optional
# Exit 0: pinned. Exit 3: unset and optional (the runs that need the runner are skipped).
# Exit 1: unset and required, or neither a release tag nor a commit SHA.
set -euo pipefail

mode="${1:?required or optional}"
ref="${ISOTHRUM_REF:-}"
help="an Isothrum release tag (vX.Y.Z) or commit SHA; docs/dev/ci-real-hosts.md"

if [ -z "$ref" ]; then
  if [ "$mode" = optional ]; then
    echo "::notice title=Isothrum runs skipped::no ISOTHRUM_REF ($help)"
    exit 3
  fi
  echo "::error title=ISOTHRUM_REF::not set ($help)"
  exit 1
fi
if [[ "$ref" =~ ^([0-9a-f]{40}|v[0-9]+[.][0-9]+[.][0-9]+([-+][0-9A-Za-z.-]+)?)$ ]]; then
  exit 0
fi
echo "::error title=ISOTHRUM_REF::${ref} is neither a release tag nor a commit SHA ($help)"
exit 1
