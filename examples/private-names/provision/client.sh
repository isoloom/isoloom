#!/bin/sh
# The client as a native VM: curl, to reach the web server by its names.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq curl
