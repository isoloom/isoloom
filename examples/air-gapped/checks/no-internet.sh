#!/bin/sh
# From the environment's network, the internet must NOT answer.
set -u
if curl -fsS --max-time 5 -o /dev/null http://1.1.1.1/ 2>/dev/null; then
  echo "the internet answered: the network isn't offline" >&2
  exit 1
fi
echo "no internet from the environment"
