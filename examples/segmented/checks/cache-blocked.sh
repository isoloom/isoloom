#!/bin/sh
# From the access network, the cache must NOT answer: the router drops it.
set -u
if nc -z -w 5 cache 6379 2>/dev/null; then
  echo "cache answered from access: the network isn't isolated" >&2
  exit 1
fi
echo "cache is blocked from access"
