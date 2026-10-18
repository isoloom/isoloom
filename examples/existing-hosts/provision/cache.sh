#!/bin/sh
# Redis on a machine that already exists, run over SSH by `isothrum up .`.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq redis-server
sed -i 's/^bind .*/bind 0.0.0.0/; s/^protected-mode .*/protected-mode no/' /etc/redis/redis.conf
systemctl restart redis-server
