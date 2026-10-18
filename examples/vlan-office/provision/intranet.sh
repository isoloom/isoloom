#!/usr/bin/env sh
set -eu
apt-get update
apt-get install -y nginx
systemctl enable --now nginx
