#!/bin/sh
# Redis on the LAN (installed before the machine's traffic moves behind the firewall).
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq redis-server
sed -i 's/^bind .*/bind 0.0.0.0/; s/^protected-mode .*/protected-mode no/' /etc/redis/redis.conf
systemctl restart redis-server
