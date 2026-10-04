#!/bin/sh
# From outside, the cache on the LAN must NOT answer: the firewall drops it.
set -u
if nc -z -w 5 cache 6379 2>/dev/null; then
  echo "cache answered from outside: the firewall lets the LAN through" >&2
  exit 1
fi
echo "the LAN is blocked from outside"
