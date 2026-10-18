#!/bin/sh
# nslookup, for the check that resolves a name through the DNS server.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq dnsutils
