#!/bin/sh
# nginx on a machine that already exists, run over SSH by `isoloom run external`.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq nginx
systemctl restart nginx
