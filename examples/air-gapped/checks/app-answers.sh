#!/bin/sh
# The app answers on the environment's network.
set -eu
curl -fsS --max-time 10 -o /dev/null http://app/
echo "app answers"
