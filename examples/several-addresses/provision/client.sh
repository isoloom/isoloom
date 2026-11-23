#!/bin/sh
# The client: curl and ping, to reach the web server and the block host at each address.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq curl iputils-ping
