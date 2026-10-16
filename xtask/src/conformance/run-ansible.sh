#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# One conformance run that proves the Ansible renderer's parity: real Ansible applies the generated
# role (the first apply), then applies it again (the second apply) and must change nothing. The
# probe is the shell renderer's own, so a pass means Ansible reached the state the shell reaches.
# From the project at /opt/isoloom and the role and `site.yml` at /isoloom-test. `cargo xtask
# conformance` reads the lines it prints.
# Usage: run-ansible.sh <repository kind> <mode, ignored>
kind=$1
cd /opt/isoloom || exit 2

# Ansible isn't in the family's base image: install ansible-core (and, for apt, python3-debian,
# which the deb822_repository module needs) from the family's package manager.
# ca-certificates too: the get_url module verifies TLS from the system trust store, and a slim
# base image may ship none; python3-debian is what the deb822_repository module needs.
case $kind in
  apt) export DEBIAN_FRONTEND=noninteractive; apt-get update -qq >/tmp/install.log 2>&1 && apt-get install -y -qq --no-install-recommends ansible-core python3-debian ca-certificates >>/tmp/install.log 2>&1 ;;
  rpm) dnf install -y -q ansible-core ca-certificates >/tmp/install.log 2>&1 ;;
  apk) apk add --no-cache -q ansible-core ca-certificates >/tmp/install.log 2>&1 ;;
  *) echo "APPLY-FAILED unknown-kind $kind"; exit 0 ;;
esac
if ! command -v ansible-playbook >/dev/null 2>&1; then
  tail -n 20 /tmp/install.log
  echo "APPLY-FAILED ansible-install"
  exit 0
fi

# The ansible.posix collection the firewall task names (skipped in a container, but the module
# must still resolve). The run manifest's requirements.yml pins it; here Galaxy provides it.
ansible-galaxy collection install ansible.posix >/tmp/galaxy.log 2>&1 || true

cd /isoloom-test || exit 2
export ANSIBLE_HOST_KEY_CHECKING=False ANSIBLE_NOCOLOR=1 ANSIBLE_LOCALHOST_WARNING=False ANSIBLE_SYSTEM_WARNINGS=False

# The first apply: the role reaches the state.
if ! ansible-playbook -i 'localhost,' -c local site.yml >/tmp/first.log 2>&1; then
  tail -n 25 /tmp/first.log
  echo "APPLY-FAILED first"
fi
sh /isoloom-test/probe.sh "$kind" | sed 's/^/FIRST /'

# The second apply: idempotent, so the PLAY RECAP reports no changed task.
if ! ansible-playbook -i 'localhost,' -c local site.yml >/tmp/second.log 2>&1; then
  tail -n 25 /tmp/second.log
  echo "APPLY-FAILED second"
fi
grep -E 'changed=[1-9]' /tmp/second.log | sed 's/^/SECOND /'
sh /isoloom-test/probe.sh "$kind" | sed 's/^/AGAIN /'
echo "END"
