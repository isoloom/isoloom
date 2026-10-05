#!/bin/sh
# The intranet as a VM: the same nginx page the container serves.
set -e
command -v nginx >/dev/null || { apt-get update -q && apt-get install -yq nginx; }
systemctl enable --now nginx
