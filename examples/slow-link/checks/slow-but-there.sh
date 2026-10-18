#!/bin/sh
# The far site answers, and the link's delay shows: a request takes at least the one-way delay
# added by the router (80 ms), where a local answer would take a few milliseconds.
set -eu
t=$(curl -s -o /dev/null --max-time 10 -w '%{time_total}' http://web/)
echo "request took ${t}s"
awk -v t="$t" 'BEGIN { exit !(t >= 0.07) }' || { echo "too fast: the link isn't impaired" >&2; exit 1; }
