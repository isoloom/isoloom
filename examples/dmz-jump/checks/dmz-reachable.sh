#!/bin/sh
# From the access network, the gateway in the DMZ answers on its web and SSH ports.
set -eu
nc -z -w 5 gateway 80
nc -z -w 5 gateway 22
echo "the dmz answers"
