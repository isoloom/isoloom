#!/bin/sh
# The web container answers on the lab network.
set -e
curl -fsS -o /dev/null http://10.60.0.10/ && echo "✓ web answers"
