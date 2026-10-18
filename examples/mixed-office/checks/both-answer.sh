#!/bin/sh
# Both machines answer on the office network, whatever each runs as.
set -e
curl -fsS -o /dev/null http://192.168.58.20/ && echo "✓ intranet answers"
timeout 5 bash -c 'exec 3<>/dev/tcp/192.168.58.10/445'
echo "✓ files01 shares on 445"
