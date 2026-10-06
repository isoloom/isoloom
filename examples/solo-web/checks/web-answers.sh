#!/bin/sh
# The page answers on the web server's port.
set -eu
wget -qO- "http://web:80/" | grep -q "hello from isoloom"
