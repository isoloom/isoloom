#!/bin/sh
# From web01 itself, on the DMZ and corp networks: dc01's directory answers on LDAP.
set -eu
timeout 5 bash -c 'exec 3<>/dev/tcp/10.40.20.5/389'
echo "web01 reaches dc01's LDAP"
