#!/bin/sh
# From the access network, the web server answers (through the router).
set -eu
curl -fsS --max-time 10 http://web/ | grep -q "web on the front network"
echo "web is reachable from access"
