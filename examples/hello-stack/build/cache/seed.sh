#!/bin/sh
# Seeds the greeting the checks look for (runs once, after the cache answers).
set -eu
redis-cli -h cache SET greeting "hello from isoloom"
