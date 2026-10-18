#!/bin/sh
# dnsmasq as a native VM, with the container's zone (installed before this machine goes offline).
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq dnsmasq
cp build/dns/lab.conf /etc/dnsmasq.d/lab.conf
systemctl restart dnsmasq
