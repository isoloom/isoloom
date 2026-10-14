#!/bin/sh
# The web server as a native VM: nginx.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq nginx
systemctl restart nginx
