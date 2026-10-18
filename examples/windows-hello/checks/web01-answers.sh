#!/bin/sh
# IIS answers with the machine's page.
set -eu
curl -fsS --max-time 10 http://web01/ | grep -qi "hello from web01"
echo "web01 answers"
