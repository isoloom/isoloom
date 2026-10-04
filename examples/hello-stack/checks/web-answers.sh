#!/bin/sh
# The page answers on web:80 with its greeting.
set -eu
curl -fsS --max-time 10 http://web/ | grep -q "hello from isoloom"
echo "web answers"
