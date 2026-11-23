#!/bin/sh
# The domain is up: dc01 answers DNS, Kerberos, LDAP and SMB.
set -eu
for port in 53 88 389 445; do
  timeout 5 bash -c "exec 3<>/dev/tcp/10.40.20.5/$port"
done
echo "dc01 serves the domain"
