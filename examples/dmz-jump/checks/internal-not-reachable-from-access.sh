#!/bin/sh
# From the access network, nothing behind the DMZ answers: `reach` opens only the DMZ to it.
set -eu
for target in app:8080 db:5432; do
  if nc -z -w 5 "${target%:*}" "${target#*:}" 2>/dev/null; then
    echo "$target answers from the access network" >&2
    exit 1
  fi
done
echo "the internal network is out of reach"
