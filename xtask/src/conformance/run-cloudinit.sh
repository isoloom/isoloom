#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# One conformance run that proves the cloud-init carrier's parity: real cloud-init writes and
# runs the recipe (the first apply), then the shell library runs on top (the second apply) and
# must change nothing. From the project at /opt/isoloom and the cloud-config at
# /isoloom-test/user-data. `cargo xtask conformance` reads the lines it prints.
# Usage: run-cloudinit.sh <repository kind> <mode, ignored>
kind=$1
cd /opt/isoloom || exit 2

# cloud-init isn't in the family's base image: install it from the family's package manager.
case $kind in
  apt) export DEBIAN_FRONTEND=noninteractive; apt-get update -qq >/tmp/install.log 2>&1 && apt-get install -y -qq --no-install-recommends cloud-init >>/tmp/install.log 2>&1 ;;
  rpm) dnf install -y -q cloud-init >/tmp/install.log 2>&1 ;;
  apk) apk add --no-cache -q cloud-init >/tmp/install.log 2>&1 ;;
  *) echo "APPLY-FAILED unknown-kind $kind"; exit 0 ;;
esac
if ! command -v cloud-init >/dev/null 2>&1; then
  tail -n 20 /tmp/install.log
  echo "APPLY-FAILED cloud-init-install"
  exit 0
fi

# The NoCloud datasource: the generated cloud-config as user data, and the stages that run it.
mkdir -p /var/lib/cloud/seed/nocloud
cp /isoloom-test/user-data /var/lib/cloud/seed/nocloud/user-data
printf 'instance-id: iid-isoloom\nlocal-hostname: m\n' > /var/lib/cloud/seed/nocloud/meta-data
{
  cloud-init init --local
  cloud-init init
  cloud-init modules --mode=config
  cloud-init modules --mode=final
} >/tmp/first.log 2>&1
if [ ! -d /var/lib/cloud/instance ]; then
  tail -n 20 /tmp/first.log
  echo "APPLY-FAILED cloud-init-run"
fi

sh /isoloom-test/probe.sh "$kind" | sed 's/^/FIRST /'

# The shell library on top: the second apply. It must change nothing (parity).
if ! sh /isoloom-test/apply.sh >/tmp/second.log 2>&1; then
  tail -n 20 /tmp/second.log
  echo "APPLY-FAILED second"
fi
grep '^isoloom: changed' /tmp/second.log | sed 's/^/SECOND /'
sh /isoloom-test/probe.sh "$kind" | sed 's/^/AGAIN /'
echo "END"
