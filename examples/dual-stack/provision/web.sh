#!/bin/sh
# nginx on the back network, answering on IPv4 and IPv6 alike (Debian's default site listens
# on both).
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq nginx
systemctl restart nginx
