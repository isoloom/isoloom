#!/bin/sh
# The cache answers on cache:6379 and holds the seeded greeting.
set -eu
printf 'GET greeting\r\nQUIT\r\n' | nc -w 5 cache 6379 | grep -q "hello from isoloom"
echo "cache is seeded"
