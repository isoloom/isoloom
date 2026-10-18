#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# One conformance run in a family's container, from the project at /opt/isoloom: the first apply
# (unless the image's build was it), the state it reached, a second apply and what it changed,
# then the state again. `cargo xtask conformance` reads the lines it prints.
# Usage: run.sh <repository kind> apply|built
cd /opt/isoloom || exit 2
if [ "$2" = apply ]; then
  if ! sh /isoloom-test/apply.sh >/tmp/first.log 2>&1; then
    tail -n 20 /tmp/first.log
    echo "APPLY-FAILED first"
    exit 0
  fi
fi
sh /isoloom-test/probe.sh "$1" | sed 's/^/FIRST /'
if ! sh /isoloom-test/apply.sh >/tmp/second.log 2>&1; then
  tail -n 20 /tmp/second.log
  echo "APPLY-FAILED second"
fi
grep '^isoloom: changed' /tmp/second.log | sed 's/^/SECOND /'
sh /isoloom-test/probe.sh "$1" | sed 's/^/AGAIN /'
echo "END"
