#!/bin/sh
# From outside, the web server in the DMZ answers: the firewall forwards port 80.
set -eu
curl -fsS --max-time 10 http://web/ | grep -q "web in the dmz"
echo "web answers through the firewall"
