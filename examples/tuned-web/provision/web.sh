#!/bin/sh
# The web server as a native VM: nginx, which systemd starts with the machine's environment
# and limits (Isoloom sets them before this step).
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq nginx
systemctl restart nginx
