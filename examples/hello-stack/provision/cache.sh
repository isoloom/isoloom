#!/bin/sh
# The cache as a native VM: Redis listening on the lab network, seeded like the container.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq redis-server
sed -i 's/^bind .*/bind 0.0.0.0/; s/^protected-mode .*/protected-mode no/' /etc/redis/redis.conf
systemctl restart redis-server
until redis-cli ping >/dev/null 2>&1; do sleep 1; done
redis-cli SET greeting "hello from isoloom"
